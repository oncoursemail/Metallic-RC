use crate::{Result,digest};
pub const GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\RecRoom";
pub const BUILD: &str = "23191908";
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Pin { pub name: &'static str, pub sha256: &'static str, pub bytes: u64 }
pub const PINS: [Pin;4] = [
    Pin{name:"RecRoom.exe",sha256:"3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70",bytes:672800},
    Pin{name:"GameAssembly.dll",sha256:"a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84",bytes:287903264},
    Pin{name:"Referee.dll",sha256:"5868c90a7c67757f3b678aa356cbd933d646c9749e0acaa31b7dcc9eaee12ead",bytes:63516208},
    Pin{name:"UnityPlayer.dll",sha256:"30f4a99c49566b2c5045469b5a39572dd2298adc31f7fe2bc39d8bb8fc9d75bb",bytes:32858656},
];
pub(crate) fn verify_record(pin: Pin, size: u64, hash: [u8;32]) -> Result<()> {
    if !PINS.contains(&pin) || size!=pin.bytes || hash!=digest(pin.sha256)? { return Err("exact_original_pin_required"); }
    Ok(())
}
pub(crate) fn command_line(private_log: &str) -> Result<String> {
    if private_log.is_empty() || private_log.chars().any(|c|c=='"'||c<' ') { return Err("private_log_path_refused"); }
    Ok(format!("\"{GAME}\\RecRoom.exe\" +forcemode:screen -screen-fullscreen 0 -screen-width 1280 -screen-height 720 -logFile \"{private_log}\""))
}

#[cfg(windows)] pub(crate) struct Originals {
    pub images: Vec<std::fs::File>,
    pub _directories: Vec<crate::process::Handle>,
}
#[cfg(windows)] impl Originals {
    pub fn verify(_permit: &crate::admission::Permit) -> Result<Self> {
        use std::{io::Read,path::Path};
        let directories=crate::process::lock_ancestors(Path::new(GAME))?;
        let mut images=Vec::new();
        for pin in PINS {
            let mut file=crate::process::lock_image(&Path::new(GAME).join(pin.name))?;
            crate::process::file_info(&file)?;
            let size=file.metadata().map_err(|_|"original_metadata_unavailable")?.len();
            if size!=pin.bytes { return Err("exact_original_size_required"); }
            let mut bytes=Vec::new(); std::io::Read::by_ref(&mut file).take(pin.bytes+1).read_to_end(&mut bytes).map_err(|_|"original_hash_read_failed")?;
            verify_record(pin,bytes.len() as u64,crate::process::hash(&bytes)?)?;
            images.push(file);
        }
        Ok(Self{images,_directories:directories})
    }
    pub fn target_image(&self) -> &std::fs::File { &self.images[0] }
    pub fn target_path(&self) -> std::path::PathBuf { std::path::Path::new(GAME).join(PINS[0].name) }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn pristine_each_original_size_digest_and_identity_is_required() {
        for pin in PINS {
            let h=digest(pin.sha256).unwrap(); assert_eq!(verify_record(pin,pin.bytes,h),Ok(()));
            assert!(verify_record(pin,pin.bytes+1,h).is_err()); let mut changed=h;changed[0]^=1;assert!(verify_record(pin,pin.bytes,changed).is_err());
            let wrong=Pin{name:"Other.exe",..pin};assert!(verify_record(wrong,pin.bytes,h).is_err());
        }
    }
    #[test] fn pristine_command_is_fixed_screen_only_and_quotes_the_private_log() {
        let line=command_line(r"C:\OwnedFixture\private-player.log").unwrap();
        assert_eq!(line,r#""C:\Program Files (x86)\Steam\steamapps\common\RecRoom\RecRoom.exe" +forcemode:screen -screen-fullscreen 0 -screen-width 1280 -screen-height 720 -logFile "C:\OwnedFixture\private-player.log""#);
        for name in ["", "quote\" -extra", "nul\0path", "line\npath"] { assert!(command_line(name).is_err()); }
    }
}
