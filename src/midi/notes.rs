use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::events::{MIDIEvent, MIDIEventType};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Note {
    pub start: u32, // in ticks
    pub length: u32, // in ticks
    pub channel: u8,
    pub key: u8,
    pub velocity: u8
}

#[derive(Hash, PartialEq, Eq)]
pub struct ProjectNote {
    pub start: u32,
    pub length: u32,
    pub channel_track: u32, // 00TTTTCC
    pub key: u8,
    pub velocity: u8,
}

pub struct ProjectNoteManager {
    pub project_notes: HashMap<u32, Arc<ProjectNote>>,
    pub curr_id: u32,

    pub render_needs_update: bool
}

impl Default for ProjectNoteManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectNoteManager {
    pub fn new() -> Self {
        Self {
            project_notes: HashMap::new(),
            curr_id: 0,
            render_needs_update: false
        }
    }

    pub fn add_note(&mut self, track: u16, note: Note) {
        let _note = ProjectNote {
            start: note.start,
            length: note.length,
            channel_track: ((track as u32) << 8) | (note.channel as u32),
            key: note.key,
            velocity: note.velocity
        };
        self.project_notes.insert(self.curr_id, Arc::new(_note));
        self.curr_id += 1;
        self.render_needs_update = true;
    }

    pub fn convert_notes(&mut self, notes: Vec<Note>) {
        for n in notes {
            let note = ProjectNote {
                start: n.start,
                length: n.length - n.start,
                channel_track: n.channel as u32,
                key: n.key,
                velocity: n.velocity
            };
            self.project_notes.insert(self.curr_id, Arc::new(note));
            self.curr_id += 1;
        }
    }

    pub fn remove_last_note(&mut self) {
        if self.project_notes.len() > 0 {
            self.project_notes.remove(&self.curr_id);
            self.curr_id -= 1;
        }
    }

    pub fn get_notes(&self) -> HashMap<usize, Vec<Arc<ProjectNote>>> {
        let mut notes = self.project_notes.values().map(|v| Arc::clone(v)).collect::<Vec<Arc<ProjectNote>>>();
        notes.sort_by_key(|n| n.start);

        let mut grouped: HashMap<usize, Vec<Arc<ProjectNote>>> = HashMap::new();
        for note in notes {
            grouped.entry(((note.channel_track >> 16) & 0xFFFF) as usize).or_default().push(note)
        }

        return grouped;
    }

    pub fn get_events(&mut self) -> Vec<MIDIEvent> {
        let mut events = Vec::new();

        for note in self.project_notes.values() {
            let ch = (note.channel_track & 0xFF) as u8;

            events.push(
                MIDIEvent {
                    time: note.start as f32,
                    event_type: MIDIEventType::NoteOn,
                    data: vec![
                        0x90 | (ch & 0x0F),
                        note.key,
                        note.velocity
                    ]
                }
            );

            events.push(
                MIDIEvent {
                    time: (note.start + note.length) as f32,
                    event_type: MIDIEventType::NoteOff,
                    data: vec![
                        0x80 | (ch & 0x0F),
                        note.key
                    ]
                }
            );
        }
        events.sort_by_key(|e| (e.time * 1000000.0) as u32);
        events
    }
}