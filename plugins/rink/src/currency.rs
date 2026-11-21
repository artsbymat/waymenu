use std::{
    env,
    fs::{self, File},
    io::{self, Read, Seek, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    task::Poll,
    time::Duration,
};

use rink_core::Context;

const RATES_URL: &str = "https://rinkcalc.app/data/currency.json";
const CACHE_MAX_AGE: Duration = Duration::from_secs(3600);
const MAX_RATES_BYTES: u64 = 262_144;

pub(super) struct Rates {
    cache: Option<PathBuf>,
    fetch: Fetch,
}

enum Fetch {
    Idle,
    Downloading(Download),
    Finished(Result<(), String>),
}

impl Rates {
    pub(super) fn new() -> Self {
        let cache = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".cache"))
            })
            .map(|directory| directory.join("waymenu/rink/currency.json"));
        Self {
            cache,
            fetch: Fetch::Idle,
        }
    }

    pub(super) fn poll(&mut self, context: &mut Context) -> Poll<Result<(), &str>> {
        if matches!(self.fetch, Fetch::Idle) {
            self.fetch = if self.load_cache(context, Some(CACHE_MAX_AGE)).is_some() {
                Fetch::Finished(Ok(()))
            } else {
                match Download::start() {
                    Ok(download) => Fetch::Downloading(download),
                    Err(error) => Fetch::Finished(self.use_stale_cache(context, error)),
                }
            };
        }

        if let Fetch::Downloading(download) = &mut self.fetch {
            if let Some(result) = download.poll() {
                let result = result.and_then(|data| {
                    apply_rates(context, &data)?;
                    if let Err(error) = self.save_cache(&data) {
                        eprintln!("waymenu-rink: could not save rates: {error}");
                    }
                    Ok(())
                });
                self.fetch =
                    Fetch::Finished(result.or_else(|error| self.use_stale_cache(context, error)));
            }
        }

        match &self.fetch {
            Fetch::Finished(result) => {
                Poll::Ready(result.as_ref().map(|_| ()).map_err(String::as_str))
            }
            _ => Poll::Pending,
        }
    }

    fn load_cache(&self, context: &mut Context, max_age: Option<Duration>) -> Option<()> {
        let file = File::open(self.cache.as_ref()?).ok()?;
        if let Some(max_age) = max_age {
            if file.metadata().ok()?.modified().ok()?.elapsed().ok()? >= max_age {
                return None;
            }
        }
        apply_rates(context, &read_rates(file).ok()?).ok()
    }

    fn use_stale_cache(&self, context: &mut Context, error: String) -> Result<(), String> {
        self.load_cache(context, None).ok_or(error)
    }

    fn save_cache(&self, data: &str) -> io::Result<()> {
        if let Some(path) = &self.cache {
            let directory = path
                .parent()
                .expect("currency cache has a parent directory");
            fs::create_dir_all(directory)?;
            let mut file = tempfile::NamedTempFile::new_in(directory)?;
            file.write_all(data.as_bytes())?;
            file.persist(path)?;
        }
        Ok(())
    }
}

fn apply_rates(context: &mut Context, data: &str) -> Result<(), String> {
    let definitions = rink_core::CURRENCY_FILE.ok_or("Rink currency definitions are missing")?;
    let mut updated = rink_core::simple_context()?;
    updated.load_currency(Some(data), definitions)?;
    rink_core::eval(&mut updated, "1 USD -> EUR")
        .map_err(|_| "The exchange rate data does not contain a usable USD rate")?;
    updated.save_previous_result = context.save_previous_result;
    updated.previous_result = context.previous_result.take();
    *context = updated;
    Ok(())
}

fn read_rates(file: impl Read) -> io::Result<String> {
    let mut data = String::new();
    file.take(MAX_RATES_BYTES + 1).read_to_string(&mut data)?;
    if data.len() as u64 > MAX_RATES_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "exchange rate data is too large",
        ));
    }
    Ok(data)
}

struct Download {
    child: Child,
    file: File,
}

impl Download {
    fn start() -> Result<Self, String> {
        let file = tempfile::tempfile().map_err(|error| error.to_string())?;
        let output = file.try_clone().map_err(|error| error.to_string())?;
        let child = Command::new("curl")
            .args([
                "--disable",
                "--fail",
                "--silent",
                "--proto",
                "=https",
                "--connect-timeout",
                "2",
                "--max-time",
                "4",
                "--max-filesize",
                &MAX_RATES_BYTES.to_string(),
                "--user-agent",
                concat!("Waymenu Rink plugin/", env!("CARGO_PKG_VERSION")),
                RATES_URL,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not start curl: {error}"))?;
        Ok(Self { child, file })
    }

    fn poll(&mut self) -> Option<Result<String, String>> {
        match self.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) if status.success() => Some(
                self.file
                    .rewind()
                    .and_then(|()| read_rates(&self.file))
                    .map_err(|error| error.to_string()),
            ),
            Ok(Some(status)) => Some(Err(format!("Could not download exchange rates ({status})"))),
            Err(error) => Some(Err(error.to_string())),
        }
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_cached_rates_and_keeps_them_when_a_replacement_is_invalid() {
        let directory = tempfile::tempdir().unwrap();
        let mut rates = Rates {
            cache: Some(directory.path().join("waymenu/rink/currency.json")),
            fetch: Fetch::Idle,
        };
        rates
            .save_cache(
                r#"[
            {"name":"USD","type":"unit","expr":"0.8 EUR"},
            {"name":"IDR","type":"unit","expr":"0.00005 EUR"}
        ]"#,
            )
            .unwrap();

        let mut context = crate::new_context().unwrap();
        assert!(rink_core::eval(&mut context, "100 USD -> IDR").is_err());
        assert_eq!(rates.poll(&mut context), Poll::Ready(Ok(())));
        let result = rink_core::one_line(&mut context, "100 USD -> IDR").unwrap();
        assert!(result.starts_with("1600000 IDR"), "{result}");
        assert!(apply_rates(&mut context, "[]").is_err());
        assert_eq!(
            rink_core::one_line(&mut context, "100 USD -> IDR").unwrap(),
            result
        );
    }
}
