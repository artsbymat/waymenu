use smithay_client_toolkit::seat::keyboard::{Keysym, Modifiers};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Binding {
    ctrl: bool,
    alt: bool,
    shift: bool,
    logo: bool,
    key: String,
}

impl Binding {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut binding = Self {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: String::new(),
        };
        for part in input.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => binding.ctrl = true,
                "alt" => binding.alt = true,
                "shift" => binding.shift = true,
                "super" | "logo" | "meta" => binding.logo = true,
                "" => return Err("empty keybinding component".into()),
                key if binding.key.is_empty() => {
                    let normalized = match key {
                        "del" => "delete",
                        "leftarrow" => "left",
                        "rightarrow" => "right",
                        "uparrow" => "up",
                        "downarrow" => "down",
                        other => other,
                    };
                    binding.key = normalized.to_owned();
                }
                _ => return Err(format!("keybinding {input:?} has more than one key")),
            }
        }
        if binding.key.is_empty() {
            return Err(format!("keybinding {input:?} has no key"));
        }
        Ok(binding)
    }

    pub fn matches(&self, keysym: Keysym, modifiers: Modifiers) -> bool {
        let name = keysym
            .name()
            .unwrap_or_default()
            .strip_prefix("XK_")
            .unwrap_or_else(|| keysym.name().unwrap_or_default())
            .to_ascii_lowercase();
        let name = match name.as_str() {
            "kp_enter" => "return",
            "kp_delete" => "delete",
            "kp_left" => "left",
            "kp_right" => "right",
            "kp_up" => "up",
            "kp_down" => "down",
            other => other,
        };
        self.ctrl == modifiers.ctrl
            && self.alt == modifiers.alt
            && self.shift == modifiers.shift
            && self.logo == modifiers.logo
            && self.key == name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_modifiers_case_insensitively() {
        assert_eq!(Binding::parse("Ctrl+Shift+N").unwrap().key, "n");
        assert!(Binding::parse("Ctrl+").is_err());
    }

    #[test]
    fn matches_xkb_key_names_from_wayland() {
        assert!(Binding::parse("Return")
            .unwrap()
            .matches(Keysym::Return, Modifiers::default()));
        assert!(Binding::parse("Escape")
            .unwrap()
            .matches(Keysym::Escape, Modifiers::default()));
        assert!(Binding::parse("Down")
            .unwrap()
            .matches(Keysym::Down, Modifiers::default()));
        assert!(Binding::parse("F1")
            .unwrap()
            .matches(Keysym::F1, Modifiers::default()));
        assert!(Binding::parse("Delete")
            .unwrap()
            .matches(Keysym::Delete, Modifiers::default()));
        assert!(Binding::parse("Del")
            .unwrap()
            .matches(Keysym::Delete, Modifiers::default()));
        assert!(Binding::parse("Ctrl+Delete").unwrap().matches(
            Keysym::Delete,
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
        assert!(Binding::parse("Ctrl+BackSpace").unwrap().matches(
            Keysym::BackSpace,
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
        assert!(Binding::parse("Ctrl+Left").unwrap().matches(
            Keysym::Left,
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
        assert!(Binding::parse("Ctrl+Right").unwrap().matches(
            Keysym::Right,
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
    }
}
