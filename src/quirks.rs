//! Per-game workarounds keyed on the game's executable name, in the spirit
//! of GPU driver application profiles. Environment variables always override
//! the corresponding quirk.

use std::sync::OnceLock;

#[derive(Debug, Default, Clone, Copy)]
pub struct Quirks {
    /// Don't expose generic trackers to this game. Trackers identify as Vive
    /// Trackers, which some games sniff to pick a controller scheme (e.g.
    /// SUPERHOT VR switches to its Vive scheme - where dropping items
    /// requires a trackpad - when it sees one).
    pub no_generic_trackers: bool,
}

pub fn get() -> Quirks {
    static QUIRKS: OnceLock<Quirks> = OnceLock::new();
    *QUIRKS.get_or_init(|| {
        let Some(exe) = game_exe_name() else {
            return Quirks::default();
        };

        let quirks = match exe.to_lowercase().as_str() {
            "superhotvr.exe" => Quirks {
                no_generic_trackers: true,
            },
            _ => Quirks::default(),
        };

        if quirks.no_generic_trackers {
            log::info!("Applying game quirks for {exe}: {quirks:?}");
        }
        quirks
    })
}

/// The basename of the game's executable. Wine rewrites argv[0] to the
/// Windows path of the .exe, so this works for Proton games too.
fn game_exe_name() -> Option<String> {
    let cmdline = std::fs::read_to_string("/proc/self/cmdline")
        .ok()
        .and_then(|args| {
            args.split('\0').next().map(|argv0| {
                argv0
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(argv0)
                    .to_string()
            })
        });

    cmdline
        .filter(|name| !name.is_empty())
        .or_else(|| {
            // comm is truncated to 15 characters, but it's better than nothing.
            std::fs::read_to_string("/proc/self/comm")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn exe_name_resolves() {
        // The test runner is a normal Linux process, so this should always
        // produce something.
        assert!(super::game_exe_name().is_some());
    }
}
