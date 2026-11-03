use calloop::{channel, EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{AxisScroll, PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

use crate::{
    config::{Config, Keybindings},
    keybindings::Binding,
    plugins::{execute_shell_cancellable, ShellOutput},
    ui::{DmenuModelOptions, Renderer, UiModel},
    Action,
};

pub fn run(config: Config) -> Result<Action, Box<dyn std::error::Error>> {
    run_model(UiModel::new(config.clone()), config)
}

pub fn run_with_input(config: Config, input: &str) -> Result<Action, Box<dyn std::error::Error>> {
    let mut model = UiModel::new(config.clone());
    model.insert(input);
    run_model(model, config)
}

pub struct DmenuOptions {
    pub prompt: String,
    pub initial_query: String,
    pub print_query: bool,
    pub fuzzy: bool,
    pub case_sensitive: bool,
    pub password: bool,
}

pub fn run_dmenu(
    config: Config,
    items: Vec<String>,
    options: DmenuOptions,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let model = UiModel::dmenu(
        config.clone(),
        items,
        DmenuModelOptions {
            prompt: options.prompt,
            initial_query: options.initial_query,
            print_query: options.print_query,
            fuzzy: options.fuzzy,
            case_sensitive: options.case_sensitive,
            password: options.password,
        },
    );
    match run_model(model, config)? {
        Action::CopyText(text) => Ok(Some(text)),
        _ => Ok(None),
    }
}

fn run_model(model: UiModel, config: Config) -> Result<Action, Box<dyn std::error::Error>> {
    let width = config.general.width;
    let height = config
        .general
        .height
        .unwrap_or_else(|| (110 + config.general.max_results as u32 * 58).min(900));
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init(&connection)?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|_| "compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh)?;
    // Double buffering keeps redraws responsive while bounding shared-memory usage.
    let pool = SlotPool::new((width * height * 4 * 2) as usize, &shm)?;
    let bindings = RuntimeBindings::new(&config.keybindings)?;
    let (worker_sender, worker_channel) = channel::channel::<WorkerMessage>();
    let mut event_loop: EventLoop<'static, State> = EventLoop::try_new()?;
    let mut state = State {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        layer: None,
        pool,
        width,
        height,
        configured: false,
        keyboard: None,
        pointer: None,
        scroll_accumulator: 0.0,
        modifiers: Modifiers::default(),
        bindings,
        model,
        renderer: Renderer::new(&config)?,
        action: Action::None,
        worker_sender,
        shell: config.plugins.shell.clone(),
        shell_running: false,
        shell_cancel: None,
        binary_scanning: false,
        loop_handle: event_loop.handle(),
    };
    queue.roundtrip(&mut state)?;
    let selected_output = config
        .general
        .monitor
        .as_deref()
        .map(|requested| {
            state
                .find_output(requested)
                .ok_or_else(|| format!("requested monitor {requested:?} was not found"))
        })
        .transpose()?;
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("waymenu"),
        selected_output.as_ref(),
    );
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_size(width, height);
    layer.commit();
    state.layer = Some(layer);

    WaylandSource::new(connection, queue).insert(event_loop.handle())?;
    event_loop
        .handle()
        .insert_source(worker_channel, |event, _, state| {
            if let channel::Event::Msg(message) = event {
                match message {
                    WorkerMessage::Shell(message) => {
                        state.shell_running = false;
                        state.shell_cancel = None;
                        state.model.show_shell_output(
                            &message.command,
                            &message.output,
                            message.success,
                        );
                    }
                    WorkerMessage::Binaries(binaries) => {
                        state.binary_scanning = false;
                        state.model.set_binaries(binaries);
                    }
                }
                if state.configured {
                    state.draw();
                }
            }
        })?;
    state.maybe_start_binary_scan();
    while !state.model.exit {
        event_loop.dispatch(None, &mut state)?;
    }
    Ok(state.action)
}

struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    layer: Option<LayerSurface>,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    scroll_accumulator: f64,
    modifiers: Modifiers,
    bindings: RuntimeBindings,
    model: UiModel,
    renderer: Renderer,
    action: Action,
    worker_sender: channel::Sender<WorkerMessage>,
    shell: Option<String>,
    shell_running: bool,
    shell_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    binary_scanning: bool,
    loop_handle: LoopHandle<'static, State>,
}

enum WorkerMessage {
    Shell(ShellOutput),
    Binaries(Vec<crate::binary::Binary>),
}

impl State {
    fn handle_key(&mut self, event: KeyEvent) {
        let key = event.keysym;
        let modifiers = self.modifiers;
        if self.bindings.help.matches(key, modifiers) && !self.model.is_help_mode() {
            self.model.toggle_help();
        } else if self.model.is_help_mode() {
            if self.bindings.close.matches(key, modifiers) {
                self.model.exit = true;
            } else if self.bindings.help.matches(key, modifiers) {
                self.model.toggle_help();
            } else if self.bindings.next.matches(key, modifiers)
                || self.bindings.next_arrow.matches(key, modifiers)
            {
                self.model.next();
            } else if self.bindings.previous.matches(key, modifiers)
                || self.bindings.previous_arrow.matches(key, modifiers)
            {
                self.model.previous();
            }
        } else if self.bindings.close.matches(key, modifiers) {
            if let Some(cancel) = &self.shell_cancel {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
            self.model.exit = true;
        } else if self.bindings.execute.matches(key, modifiers) {
            self.execute_selection();
        } else if self.bindings.backspace.matches(key, modifiers) {
            self.model.backspace();
        } else if self.bindings.next.matches(key, modifiers)
            || self.bindings.next_arrow.matches(key, modifiers)
        {
            self.model.next();
        } else if self.bindings.previous.matches(key, modifiers)
            || self.bindings.previous_arrow.matches(key, modifiers)
        {
            self.model.previous();
        } else if self.bindings.autocomplete.matches(key, modifiers) {
            self.model.autocomplete();
        } else if self.bindings.clear.matches(key, modifiers) {
            self.model.clear();
        } else if self.bindings.delete_word.matches(key, modifiers) {
            self.model.delete_word();
        } else if self.bindings.beginning.matches(key, modifiers) {
            self.model.move_to_beginning();
        } else if self.bindings.end.matches(key, modifiers) {
            self.model.move_to_end();
        } else if !modifiers.ctrl && !modifiers.alt && !modifiers.logo {
            if let Some(text) = event
                .utf8
                .as_deref()
                .filter(|text| !text.chars().any(char::is_control))
            {
                self.model.insert(text);
            }
        }
        self.maybe_start_binary_scan();
        if self.configured {
            self.draw();
        }
    }

    fn handle_click(&mut self, x: f64, y: f64) {
        let Some(index) =
            self.renderer
                .item_at(x, y, self.width, self.height, self.model.results.len())
        else {
            return;
        };
        self.model.select(index);
        if self.model.is_help_mode() {
            if self.configured {
                self.draw();
            }
        } else {
            self.execute_selection();
        }
    }

    fn handle_axis(&mut self, vertical: AxisScroll) {
        let steps = calculate_axis_scroll(&mut self.scroll_accumulator, vertical);
        if steps > 0 {
            self.model.scroll_down_by(steps as usize);
        } else if steps < 0 {
            self.model.scroll_up_by((-steps) as usize);
        }
        self.maybe_start_binary_scan();
        if self.configured {
            self.draw();
        }
    }

    fn find_output(&self, requested: &str) -> Option<wl_output::WlOutput> {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        if let Ok(index) = requested.parse::<usize>() {
            return outputs.get(index).cloned();
        }
        outputs.into_iter().find(|output| {
            self.output_state.info(output).is_some_and(|info| {
                info.name.as_deref() == Some(requested)
                    || info.description.as_deref() == Some(requested)
                    || info.model == requested
            })
        })
    }

    fn draw(&mut self) {
        let Some(layer) = &self.layer else { return };
        let stride = self.width as i32 * 4;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            self.width as i32,
            self.height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        self.renderer
            .draw(canvas, self.width, self.height, &self.model);
        convert_rgba_to_wayland_argb8888(canvas);
        layer
            .wl_surface()
            .damage_buffer(0, 0, self.width as i32, self.height as i32);
        if buffer.attach_to(layer.wl_surface()).is_ok() {
            layer.commit();
        }
    }

    fn execute_selection(&mut self) {
        let action = self.model.selected_action();
        let Action::RunShell(command) = action else {
            self.action = action;
            self.model.exit = true;
            return;
        };
        if self.shell_running {
            return;
        }
        self.shell_running = true;
        self.model.show_shell_running(&command);
        let sender = self.worker_sender.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.shell_cancel = Some(cancel.clone());
        let environment_shell = std::env::var("SHELL").ok();
        let shell = self
            .shell
            .clone()
            .or(environment_shell)
            .unwrap_or_else(|| "/bin/sh".into());
        std::thread::spawn(move || {
            let message = execute_shell_cancellable(shell, command, cancel);
            let _ = sender.send(WorkerMessage::Shell(message));
        });
    }

    fn maybe_start_binary_scan(&mut self) {
        if !self.model.needs_binary_scan() || self.binary_scanning {
            return;
        }
        self.binary_scanning = true;
        let sender = self.worker_sender.clone();
        std::thread::spawn(move || {
            let _ = sender.send(WorkerMessage::Binaries(crate::binary::discover()));
        });
    }
}

fn calculate_axis_scroll(accumulator: &mut f64, vertical: AxisScroll) -> i32 {
    if vertical.discrete != 0 {
        *accumulator = 0.0;
        return vertical.discrete;
    }
    if vertical.stop {
        *accumulator = 0.0;
        return 0;
    }
    if vertical.absolute != 0.0 {
        *accumulator += vertical.absolute;
        const SCROLL_THRESHOLD: f64 = 15.0;
        let mut steps = 0;
        while *accumulator >= SCROLL_THRESHOLD {
            steps += 1;
            *accumulator -= SCROLL_THRESHOLD;
        }
        while *accumulator <= -SCROLL_THRESHOLD {
            steps -= 1;
            *accumulator += SCROLL_THRESHOLD;
        }
        return steps;
    }
    0
}

fn convert_rgba_to_wayland_argb8888(canvas: &mut [u8]) {
    for pixel in canvas.as_chunks_mut::<4>().0 {
        #[cfg(target_endian = "little")]
        pixel.swap(0, 2);
        #[cfg(target_endian = "big")]
        pixel.rotate_right(1);
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.model.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if configure.new_size.0 > 0 {
            self.width = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.height = configure.new_size.1;
        }
        if !self.configured {
            self.configured = true;
            self.draw();
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self
                .seat_state
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(|state, _, event| state.handle_key(event)),
                )
                .ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.handle_key(event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let Some(layer) = &self.layer {
                if &event.surface != layer.wl_surface() {
                    continue;
                }
            }
            match &event.kind {
                PointerEventKind::Press { button, .. } => {
                    if *button == BTN_LEFT {
                        self.handle_click(event.position.0, event.position.1);
                    }
                }
                PointerEventKind::Axis { vertical, .. } => {
                    self.handle_axis(*vertical);
                }
                _ => {}
            }
        }
    }
}

struct RuntimeBindings {
    execute: Binding,
    close: Binding,
    autocomplete: Binding,
    next: Binding,
    previous: Binding,
    clear: Binding,
    next_arrow: Binding,
    previous_arrow: Binding,
    backspace: Binding,
    delete_word: Binding,
    beginning: Binding,
    end: Binding,
    help: Binding,
}

impl RuntimeBindings {
    fn new(value: &Keybindings) -> Result<Self, Box<dyn std::error::Error>> {
        let parse = |input: &str| {
            Binding::parse(input)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
        };
        Ok(Self {
            execute: parse(&value.execute)?,
            close: parse(&value.close)?,
            autocomplete: parse(&value.autocomplete)?,
            next: parse(&value.next)?,
            previous: parse(&value.previous)?,
            clear: parse(&value.clear)?,
            next_arrow: parse(&value.next_arrow)?,
            previous_arrow: parse(&value.previous_arrow)?,
            backspace: parse(&value.backspace)?,
            delete_word: parse(&value.delete_word)?,
            beginning: parse(&value.beginning)?,
            end: parse(&value.end)?,
            help: parse(&value.help)?,
        })
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);
impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

#[cfg(test)]
mod tests {
    use super::{calculate_axis_scroll, convert_rgba_to_wayland_argb8888, AxisScroll};

    #[test]
    fn renderer_pixels_are_swizzled_for_wayland_shm_channel_order() {
        let mut pixel = [10, 20, 30, 255];
        convert_rgba_to_wayland_argb8888(&mut pixel);
        #[cfg(target_endian = "little")]
        assert_eq!(pixel, [30, 20, 10, 255]);
        #[cfg(target_endian = "big")]
        assert_eq!(pixel, [255, 10, 20, 30]);
    }

    #[test]
    fn axis_scroll_prioritizes_discrete_notches() {
        let mut acc = 5.0;
        let event = AxisScroll {
            absolute: 15.0,
            discrete: 2,
            stop: false,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, event), 2);
        assert_eq!(acc, 0.0);

        let event_up = AxisScroll {
            absolute: -15.0,
            discrete: -1,
            stop: false,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, event_up), -1);
        assert_eq!(acc, 0.0);
    }

    #[test]
    fn axis_scroll_accumulates_continuous_delta_and_resets_on_stop() {
        let mut acc = 0.0;

        // Small movement below threshold
        let ev1 = AxisScroll {
            absolute: 10.0,
            discrete: 0,
            stop: false,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, ev1), 0);
        assert_eq!(acc, 10.0);

        // Movement crossing threshold
        let ev2 = AxisScroll {
            absolute: 8.0,
            discrete: 0,
            stop: false,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, ev2), 1);
        assert_eq!(acc, 3.0); // 18.0 - 15.0 = 3.0

        // Stop resets accumulator
        let ev_stop = AxisScroll {
            absolute: 0.0,
            discrete: 0,
            stop: true,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, ev_stop), 0);
        assert_eq!(acc, 0.0);

        // Negative continuous scroll
        let ev_neg = AxisScroll {
            absolute: -16.0,
            discrete: 0,
            stop: false,
        };
        assert_eq!(calculate_axis_scroll(&mut acc, ev_neg), -1);
        assert_eq!(acc, -1.0);
    }
}
