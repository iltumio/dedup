use std::path::Path;

/// Hand off an extracted file to the desktop's registered application.
#[cfg(target_os = "linux")]
pub fn launch(path: &Path) -> Result<(), String> {
    // xdg-open's generic launcher can run Terminal=true applications directly,
    // leaving an invisible editor attached to the GUI's standard streams. GIO
    // respects desktop-entry activation, including launching a terminal.
    let uri = gio::glib::filename_to_uri(path, None).map_err(|error| error.to_string())?;
    gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn launch(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|error| error.to_string())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    // Run the desktop launcher in an isolated process: GIO caches MIME settings,
    // and changing process-wide environment variables would race other tests.
    #[test]
    fn launch_fixture_child() {
        if let Some(path) = std::env::var_os("DEDUP_LAUNCH_FIXTURE") {
            assert_eq!(can_open(Path::new("report.log")), Some(true));
            assert_eq!(can_open(Path::new(&path)), Some(true));
            launch(Path::new(&path)).unwrap();
        }
    }

    #[test]
    fn terminal_association_is_launched_in_a_terminal() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let data = tmp.path().join("data");
        let apps = data.join("applications");
        let config = tmp.path().join("config");
        for dir in [&bin, &apps, &config] {
            fs::create_dir_all(dir).unwrap();
        }
        let marker = tmp.path().join("terminal-args");
        let editor_marker = tmp.path().join("editor-args");
        for (name, marker) in [
            ("xdg-terminal-exec", &marker),
            ("dedup-test-editor", &editor_marker),
        ] {
            let script = bin.join(name);
            fs::write(
                &script,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                    marker.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(apps.join("dedup-test.desktop"), "[Desktop Entry]\nType=Application\nName=Dedup test\nExec=dedup-test-editor %f\nTerminal=true\nMimeType=text/plain;\n").unwrap();
        let associations = "[Default Applications]\ntext/plain=dedup-test.desktop\n";
        fs::write(config.join("mimeapps.list"), associations).unwrap();
        fs::write(apps.join("mimeapps.list"), associations).unwrap();
        let file = tmp.path().join("report with spaces & symbols.txt");
        fs::write(&file, "fixture\n").unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "file_open::tests::launch_fixture_child",
                "--nocapture",
            ])
            .env("DEDUP_LAUNCH_FIXTURE", &file)
            .env("XDG_DATA_HOME", &data)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_CURRENT_DESKTOP", "dedup-test")
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .status()
            .unwrap();
        assert!(status.success());
        // Desktop activation may return before the terminal process writes.
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            marker.exists(),
            "Terminal=true was ignored: editor launched without a terminal"
        );
        let args = fs::read_to_string(marker).unwrap();
        assert!(args.lines().any(|arg| arg == "dedup-test-editor"));
        assert!(
            args.lines().any(|arg| arg == file.to_str().unwrap()),
            "filename must remain one argument: {args}"
        );
        assert!(
            !editor_marker.exists(),
            "editor should be delegated to the terminal"
        );
    }
}

/// Check filename-based desktop associations without extracting large files.
/// None means the type/platform cannot be checked reliably, not unsupported.
#[cfg(target_os = "linux")]
pub fn can_open(path: &Path) -> Option<bool> {
    let (content_type, uncertain) = gio::content_type_guess(Some(path), &[]);
    if uncertain {
        return None;
    }
    Some(gio::AppInfo::default_for_type(&content_type, false).is_some())
}

#[cfg(not(target_os = "linux"))]
pub fn can_open(_path: &Path) -> Option<bool> {
    None
}
