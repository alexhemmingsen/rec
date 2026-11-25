//! The short sound Rec plays when you press the shortcut.
//! Made by `assets/make-sound.py`; bundled into the binary.

use tauri::AppHandle;

static SHORTCUT: &[u8] = include_bytes!("../sounds/shortcut.wav");

/// Play the shortcut sound without waiting for it. Failures are only logged:
/// the sound is a nicety.
pub fn play_shortcut(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(macos::play);
    }
    #[cfg(windows)]
    {
        let _ = app;
        windows::play();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = app;
        std::thread::spawn(linux::play);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::OnceCell;

    use objc2::AllocAnyThread;
    use objc2::rc::Retained;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;

    thread_local! {
        // Kept alive so it isn't freed while playing.
        static SOUND: OnceCell<Option<Retained<NSSound>>> = const { OnceCell::new() };
    }

    pub fn play() {
        SOUND.with(|cell| {
            let sound = cell.get_or_init(|| {
                let data = NSData::with_bytes(super::SHORTCUT);
                let sound = NSSound::initWithData(NSSound::alloc(), &data);
                if sound.is_none() {
                    log::warn!("cannot load the shortcut sound");
                }
                sound
            });
            if let Some(sound) = sound {
                sound.stop();
                sound.play();
            }
        });
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u8, module: *mut c_void, flags: u32) -> i32;
    }

    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;

    pub fn play() {
        // SAFETY: the sound is a static WAV image, so it outlives the
        // asynchronous playback SND_MEMORY reads from.
        let ok = unsafe {
            PlaySoundW(
                super::SHORTCUT.as_ptr(),
                std::ptr::null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            )
        };
        if ok == 0 {
            log::warn!("cannot play the shortcut sound");
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    /// Hand the file to whichever player the desktop has.
    pub fn play() {
        let path = std::env::temp_dir().join("rec-shortcut.wav");
        if !path.exists() && std::fs::write(&path, super::SHORTCUT).is_err() {
            return;
        }
        for player in ["pw-play", "paplay", "aplay"] {
            let mut cmd = std::process::Command::new(player);
            if player == "aplay" {
                cmd.arg("-q");
            }
            if let Ok(mut child) = cmd.arg(&path).spawn() {
                let _ = child.wait();
                return;
            }
        }
        log::warn!("no audio player found for the shortcut sound");
    }
}
