pub struct TempoEvent {
    pub time: u64,
    pub time_norm: f32,
    pub tempo: f32,
}

impl TempoEvent {
}

#[derive(Debug, Clone, Copy)]
pub enum MIDIEventType {
    NoteOff,
    NoteOn
}

#[derive(Debug, Clone)]
pub struct MIDIEvent {
    pub time: f32,
    pub event_type: MIDIEventType,
    pub data: Vec<u8>
}