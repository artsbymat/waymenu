use std::{
    io::{self, Read},
    time::{Duration, Instant},
};

use calloop::PostAction;
use smithay_client_toolkit::{
    data_device_manager::{
        data_device::{DataDeviceData, DataDeviceHandler},
        data_offer::{DataOfferData, DataOfferHandler, DragOffer},
        DataDeviceManagerState,
    },
    globals::GlobalData,
};
use wayland_client::{
    delegate_dispatch,
    protocol::{
        wl_data_device::WlDataDevice,
        wl_data_device_manager::{DndAction, WlDataDeviceManager},
        wl_data_offer::WlDataOffer,
        wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};

use super::State;

const MAX_PASTE_BYTES: usize = 64 * 1024;
pub(super) const PASTE_TIMEOUT: Duration = Duration::from_secs(2);

impl State {
    pub(super) fn paste(&mut self) {
        self.cancel_paste();
        let Some(offer) = self
            .data_device
            .as_ref()
            .and_then(|device| device.data().selection_offer())
        else {
            return;
        };
        let Some(mime) = offer.with_mime_types(preferred_text_mime) else {
            return;
        };
        let Ok(pipe) = offer.receive(mime) else {
            return;
        };
        let mut bytes = Vec::new();
        let transfer = self.loop_handle.insert_source(pipe, move |_, file, state| {
            let result = read_chunk(&mut &**file, &mut bytes);
            if matches!(result, Ok(false)) {
                return PostAction::Continue;
            }
            state.paste_transfer = None;
            if result.is_ok() && !state.model.exit {
                let text = single_line_text(&bytes);
                if !text.is_empty() {
                    state.model.insert(&text);
                    state.reset_cursor_blink();
                    state.maybe_start_binary_scan();
                    if state.configured {
                        state.draw();
                    }
                }
            }
            PostAction::Remove
        });
        self.paste_transfer = transfer.ok().map(|token| (token, Instant::now()));
    }

    pub(super) fn cancel_paste(&mut self) {
        if let Some((token, _)) = self.paste_transfer.take() {
            self.loop_handle.remove(token);
        }
    }
}

fn preferred_text_mime(types: &[String]) -> Option<String> {
    [
        "text/plain;charset=utf-8",
        "text/plain; charset=utf-8",
        "UTF8_STRING",
        "text/plain",
    ]
    .iter()
    .find_map(|preferred| {
        types
            .iter()
            .find(|mime| mime.eq_ignore_ascii_case(preferred))
    })
    .cloned()
}

fn read_chunk(reader: &mut impl Read, bytes: &mut Vec<u8>) -> io::Result<bool> {
    // Read once per readiness event because the Wayland pipe is blocking.
    let mut chunk = [0; 4096];
    match reader.read(&mut chunk) {
        Ok(0) => Ok(true),
        Ok(size) if bytes.len() + size > MAX_PASTE_BYTES => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "clipboard text is too large",
        )),
        Ok(size) => {
            bytes.extend_from_slice(&chunk[..size]);
            Ok(false)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn single_line_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut result = String::with_capacity(text.len());
    let mut chars = text.trim_end_matches(['\r', '\n']).chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                chars.next_if_eq(&'\n');
                result.push(' ');
            }
            '\t' | '\n' | '\u{2028}' | '\u{2029}' => result.push(' '),
            ch if !ch.is_control() => result.push(ch),
            _ => {}
        }
    }
    result
}

impl DataDeviceHandler for State {
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &WlSurface,
    ) {
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

delegate_dispatch!(State: [WlDataDeviceManager: GlobalData] => DataDeviceManagerState);
delegate_dispatch!(State: [WlDataDevice: DataDeviceData] => DataDeviceManagerState);
delegate_dispatch!(State: [WlDataOffer: DataOfferData] => DataDeviceManagerState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_utf8_and_ignores_nontext_offers() {
        let types = [
            "image/png".into(),
            "text/plain".into(),
            "text/plain;charset=UTF-8".into(),
        ];
        assert_eq!(
            preferred_text_mime(&types).as_deref(),
            Some("text/plain;charset=UTF-8")
        );
        assert!(preferred_text_mime(&types[..1]).is_none());
    }

    #[test]
    fn buffers_until_eof_and_normalizes_multiline_text() {
        let mut bytes = Vec::new();
        let mut input = "café\r\n水\t☕\0\n".as_bytes();
        assert!(!read_chunk(&mut input, &mut bytes).unwrap());
        assert!(read_chunk(&mut input, &mut bytes).unwrap());
        assert_eq!(single_line_text(&bytes), "café 水 ☕");
    }

    #[test]
    fn rejects_oversized_clipboard_data() {
        let mut bytes = vec![b'x'; MAX_PASTE_BYTES];
        assert!(read_chunk(&mut &b"x"[..], &mut bytes).is_err());
        assert_eq!(bytes.len(), MAX_PASTE_BYTES);
    }
}
