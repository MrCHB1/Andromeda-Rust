pub struct Navigation {
    pub tick_pos: f32,
    pub key_pos: f32,
    pub zoom_ticks: f32,
    pub zoom_keys: f32,
}

impl Navigation {
    pub fn new() -> Self {
        Self {
            tick_pos: 0.0,
            key_pos: 21.0,
            zoom_ticks: 7680.0,
            zoom_keys: 88.0,
        }
    }

    pub fn change_tick_pos(&mut self, tick_pos: f32, mut change_fn: impl FnMut(f32)) {
        self.tick_pos = tick_pos;
        change_fn(self.tick_pos);
    }
}