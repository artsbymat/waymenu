use std::{
    env,
    io::{self, BufRead},
    process::{Command, ExitCode},
};
use waymenu::{
    applications, binary, config::Config, history::History, search, Action, SearchResult,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("waymenu: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("waymenu {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--dmenu") {
        return dmenu(&args);
    }
    let query = option_string(&args, "--query")?.map(str::to_owned);
    let mode = option_string(&args, "--mode")?;
    if mode.is_some_and(|mode| mode != "binary" && mode != "applications") {
        return Err("--mode must be 'applications' or 'binary'".into());
    }
    let mut config = Config::load_or_create()?;
    if let Some(monitor) = option_string(&args, "--monitor")? {
        config.general.monitor = Some(monitor.to_owned());
    }
    if let Some(query) = query {
        let query = if mode.is_some_and(|mode| mode == "binary")
            && !query.starts_with(&config.binary.prefix)
        {
            format!("{}{query}", config.binary.prefix)
        } else {
            query
        };
        return dispatch(&query, &config);
    }
    let action = if mode.is_some_and(|mode| mode == "binary") {
        let prefix = config.binary.prefix.clone();
        waymenu::wayland::run_with_input(config.clone(), &prefix)?
    } else {
        waymenu::wayland::run(config.clone())?
    };
    execute_action(action, &config)
}

fn execute_action(action: Action, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        Action::LaunchApplication(id) => {
            applications::launch_by_id(&id, config.general.terminal.as_deref())?;
            if config.general.remember_history && config.history.enabled {
                let mut history = History::load_with_limit(config.history.max_entries);
                if let Err(error) = history.record(&id) {
                    eprintln!("waymenu: could not update history: {error}");
                }
            }
        }
        Action::LaunchBinary(path) => {
            Command::new(path).spawn()?;
        }
        Action::RunShell(command) => {
            let shell = config
                .plugins
                .shell
                .clone()
                .or_else(|| env::var("SHELL").ok())
                .unwrap_or_else(|| "/bin/sh".into());
            Command::new(shell).args(["-c", &command]).spawn()?;
        }
        Action::CopyText(_) | Action::None => {}
    }
    Ok(())
}

fn dispatch(query: &str, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(command) = query.strip_prefix(&config.plugins.shell_prefix) {
        if command.trim().is_empty() {
            return Ok(());
        }
        let shell = config
            .plugins
            .shell
            .clone()
            .or_else(|| env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".into());
        let status = Command::new(shell).args(["-c", command]).status()?;
        if !status.success() {
            return Err(format!("command exited with {status}").into());
        }
        return Ok(());
    }
    if let Some(query) = query.strip_prefix(&config.binary.prefix) {
        let binaries = binary::discover();
        let results: Vec<_> = binaries.iter().map(|item| item.as_result()).collect();
        let mut matches = search::filter(&results, query, config.general.max_results);
        if !config.binary.show_path {
            for result in &mut matches {
                result.description.clear();
            }
        }
        print_results(matches);
        return Ok(());
    }
    let mut history = History::load_with_limit(config.history.max_entries);
    let applications = applications::discover();
    let results: Vec<_> = applications
        .iter()
        .map(|app| {
            app.as_result(
                if config.general.remember_history && config.history.enabled {
                    history.score(&app.id)
                } else {
                    0
                },
            )
        })
        .collect();
    let matches = search::filter(&results, query, config.general.max_results);
    print_results(matches.clone());
    // Headless engine mode launches only when explicitly selecting an exact result.
    if let Some(id) = env::var_os("WAYMENU_EXECUTE_ID") {
        if let Some(app) = applications
            .iter()
            .find(|app| std::ffi::OsStr::new(&app.id) == id.as_os_str())
        {
            app.launch(config.general.terminal.as_deref())?;
            if config.general.remember_history && config.history.enabled {
                if let Err(error) = history.record(&app.id) {
                    eprintln!("waymenu: could not update history: {error}");
                }
            }
        }
    }
    Ok(())
}

fn print_results(results: Vec<SearchResult>) {
    for result in results {
        println!("{}\t{}\t{}", result.id, result.title, result.description);
    }
}

fn dmenu(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let prompt = option_string(args, "--prompt")?;
    let items: Vec<_> = io::stdin().lock().lines().collect::<Result<_, _>>()?;
    let query = args
        .iter()
        .position(|a| a == "--query")
        .and_then(|i| args.get(i + 1));
    if query.is_none() {
        let mut config = Config::load_or_create()?;
        if let Some(monitor) = option_string(args, "--monitor")? {
            config.general.monitor = Some(monitor.to_owned());
        }
        if let Some(lines) = option_usize(args, "--lines")? {
            config.general.max_results = lines.clamp(1, 100);
        }
        if let Some(width) = option_usize(args, "--width")? {
            config.general.width = u32::try_from(width)?.clamp(240, 4096);
        }
        if let Some(height) = option_usize(args, "--height")? {
            config.general.height = Some(u32::try_from(height)?.clamp(120, 2160));
        }
        let options = waymenu::wayland::DmenuOptions {
            prompt: prompt
                .map(str::to_owned)
                .unwrap_or_else(|| "Select…".into()),
            initial_query: String::new(),
            print_query: args.iter().any(|a| a == "--print-query"),
            fuzzy: !args.iter().any(|a| a == "--no-fuzzy"),
            case_sensitive: args.iter().any(|a| a == "--case-sensitive"),
            password: args.iter().any(|a| a == "--password"),
        };
        config.validate()?;
        if let Some(selected) = waymenu::wayland::run_dmenu(config, items, options)? {
            println!("{selected}");
        }
        return Ok(());
    }
    let query = query.cloned().unwrap_or_default();
    let mut config = Config::default();
    config.general.max_results = 1;
    let model = waymenu::ui::UiModel::dmenu(
        config,
        items,
        waymenu::ui::DmenuModelOptions {
            prompt: prompt.map(str::to_owned).unwrap_or_default(),
            initial_query: query,
            print_query: args.iter().any(|a| a == "--print-query"),
            fuzzy: !args.iter().any(|a| a == "--no-fuzzy"),
            case_sensitive: args.iter().any(|a| a == "--case-sensitive"),
            password: args.iter().any(|a| a == "--password"),
        },
    );
    if let Action::CopyText(selected) = model.selected_action() {
        println!("{selected}");
    }
    Ok(())
}

fn option_usize(args: &[String], name: &str) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    args.iter()
        .position(|arg| arg == name)
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| format!("{name} requires a value"))?
                .parse::<usize>()
                .map_err(|_| format!("invalid numeric value for {name}").into())
        })
        .transpose()
}

fn option_string<'a>(
    args: &'a [String],
    name: &str,
) -> Result<Option<&'a str>, Box<dyn std::error::Error>> {
    args.iter()
        .position(|arg| arg == name)
        .map(|index| {
            args.get(index + 1)
                .map(String::as_str)
                .ok_or_else(|| format!("{name} requires a value").into())
        })
        .transpose()
}

fn print_help() {
    println!("waymenu {}\n\nUsage: waymenu [OPTIONS]\n\n  --query TEXT       Search without opening a UI\n  --mode MODE        Open applications or binary mode\n  --dmenu            Read newline-separated choices from stdin\n  --prompt TEXT      Prompt used with --dmenu\n  --lines NUMBER     Maximum visible dmenu results\n  --width PIXELS     Dmenu width\n  --height PIXELS    Dmenu height\n  --monitor OUTPUT   Output name, model, description, or zero-based index\n  --password         Mask dmenu input\n  --no-fuzzy         Disable dmenu fuzzy matching\n  --case-sensitive   Match dmenu input case-sensitively\n  --print-query      Return raw query when dmenu has no match\n  --help             Show this help\n  --version          Show version\n\nPrefixes: $ searches PATH; ! executes only the explicitly submitted command.", env!("CARGO_PKG_VERSION"));
}
