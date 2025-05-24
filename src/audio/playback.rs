use std::time::Instant;

use crate::midi::events::TempoEvent;

pub struct Playback {
    pub playback_secs: f32,
    pub tempo_events: Vec<TempoEvent>,

    last_pos: f32,
    pub is_playing: bool,
    time_delta: Instant,
}

impl Default for Playback {
    fn default() -> Self {
        Self::new()
    }
}

impl Playback {
    pub fn new() -> Self {
        Self {
            playback_secs: 0.0,
            tempo_events: Vec::new(),
            last_pos: 0.0,
            is_playing: false,
            time_delta: Instant::now()
        }
    }

    pub fn play_or_stop(&mut self) {
        if self.is_playing {
            self.playback_secs = self.last_pos;
            println!("Stopping");
        } else {
            self.last_pos = self.playback_secs;
            self.time_delta = Instant::now();
            println!("Playing");
        }
        self.is_playing = !self.is_playing;
    }

    pub fn navigate_to(&mut self, ppq: u16, tick: f32) {
        self.last_pos = self.tick_to_secs(ppq, tick);
        self.playback_secs = self.last_pos;
    }

    pub fn get_playback_time(&mut self, ppq: u16) -> f32 {
        let time = self.time_delta.elapsed().as_secs_f32() + self.last_pos;
        if self.tempo_events.len() == 0 {
            return time * (ppq as f32 * 120.0 / 60.0);
        }

        let mut bpm = self.tempo_events[0].tempo;
        let mut last_time = self.tempo_events[0].time_norm;
        let mut last_tick = self.tempo_events[0].time;

        for tempo in self.tempo_events.iter() {
            if tempo.time_norm > time { break; }
            last_time = tempo.time_norm;
            last_tick = tempo.time;
            bpm = tempo.tempo;
        }

        let tick_pos = (time - last_time) * (ppq as f32 * bpm / 60.0);
        return tick_pos + last_tick as f32;
    }

    fn tick_to_secs(&self, ppq: u16, tick: f32) -> f32 {
        if self.tempo_events.len() == 0 {
            return tick / (ppq as f32 * 120.0 / 60.0);
        }

        let mut last_tick = 0;
        let mut last_tempo = self.tempo_events[0].tempo;
        let mut seconds = 0.0;

        for i in 1..self.tempo_events.len() {
            let ev = &self.tempo_events[i];
            if ev.time as f32 > tick {
                break;
            }

            let delta_ticks = ev.time - last_tick;
            let us_per_qn = 60000000.0 / last_tempo;
            let sec_per_tick = us_per_qn / 1000000.0 / ppq as f32;
            seconds += delta_ticks as f32 * sec_per_tick;
            last_tick = ev.time;
            last_tempo = ev.tempo;
        }

        let delta_ticks = tick - last_tick as f32;
        let us_per_qn = 60000000.0 / last_tempo;
        let sec_per_tick = us_per_qn / 1000000.0 / ppq as f32;

        seconds += delta_ticks * sec_per_tick;

        seconds
    }
}

