pub struct ProjectSettings {
    pub initial_bpm: f32,
    pub ppq: u16
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            initial_bpm: 160.0,
            ppq: 1920
        }
    }
}