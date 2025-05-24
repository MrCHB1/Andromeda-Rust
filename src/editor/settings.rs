use std::sync::Arc;

pub struct AudioSettings {
    pub soundfont_path: String,
    pub num_layers: usize
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            soundfont_path: String::from("/assets/soundfonts/Sinufont.sf2"),
            num_layers: 5
        }
    }
}

impl AudioSettings {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn set_soundfont_path(&mut self, sf_path: String) {
        self.soundfont_path = sf_path;
    }
}

pub struct ApplicationSettings {
    pub audio_settings: AudioSettings
}

impl ApplicationSettings {
    pub fn get_audio_settings(&mut self) -> &mut AudioSettings {
        &mut self.audio_settings
    }
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        Self {
            audio_settings: Default::default()
        }
    }
}