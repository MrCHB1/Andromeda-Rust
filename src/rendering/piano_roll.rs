use std::default;
use std::collections::HashMap;

use eframe::egui::Vec2;
use eframe::{egui, glow};
use eframe::glow::{HasContext, Shader};
use std::sync::{Arc, Mutex};

use crate::editor::navigation::Navigation;
use crate::editor::project_settings::{self, ProjectSettings};
use crate::midi::notes::ProjectNote;
use crate::set_attribute;

use super::buffers::{Buffer, VertexArray};
use super::shaders::ShaderProgram;

// Note buffer settings
const NOTE_BUFFER_SIZE: usize = 4096;

// Piano Roll Background
pub type BarStart = f32;
pub type BarLength = f32;
pub type BarNumber = u32;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct RenderPianoRollBar(BarStart, BarLength, BarNumber);

// Piano Roll Notes
pub type NoteRect = [f32; 4]; // (start, length, note bottom, note top)
pub type NoteColor = [f32; 3];

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct RenderPianoRollNote(NoteRect, NoteColor);

pub type Position = [f32; 2];

#[repr(C, packed)]
pub struct Vertex(Position);

pub const QUAD_VERTICES: [Vertex; 4] = [
    Vertex([0.0, 0.0]),
    Vertex([1.0, 0.0]),
    Vertex([1.0, 1.0]),
    Vertex([0.0, 1.0])
];

const QUAD_INDICES: [u32; 6] = [
    0, 1, 3,
    1, 2, 3
];

pub trait Renderer {
    fn draw(&mut self);
    fn window_size(&mut self, size: Vec2) {}
    fn update_ppq(&mut self, ppq: u16) {}
    fn update_project_notes(&mut self, project_notes: HashMap<usize, Vec<Arc<ProjectNote>>>) {}
    fn time_changed(&mut self, time: f32) {}
}

pub struct PianoRollRenderer {
    pub navigation: Arc<Mutex<Navigation>>,
    pub window_size: Vec2<>,
    pub ppq: u16,

    pr_program: ShaderProgram,
    pr_vertex_buffer: Buffer,
    pr_vertex_array: VertexArray,
    pr_instance_buffer: Buffer,
    pr_index_buffer: Buffer,

    pr_notes_program: ShaderProgram,
    pr_notes_vbo: Buffer,
    pr_notes_vao: VertexArray,
    pr_notes_ibo: Buffer,
    pr_notes_ebo: Buffer,

    gl: Arc<glow::Context>,

    bars_render: Vec<RenderPianoRollBar>,
    render_notes: HashMap<usize, Vec<Arc<ProjectNote>>>,
    notes_render: Vec<RenderPianoRollNote>,
    note_colors: Vec<[f32; 3]>,
    last_note_start: usize,
    first_unhit_note: usize
}

impl PianoRollRenderer {
    pub fn new(nav: Arc<Mutex<Navigation>>, gl: Arc<glow::Context>) -> Self {
        // compile the shaders for piano roll idk
        unsafe {
            let pr_program = ShaderProgram::create_from_files(gl.clone(), "./shaders/piano_roll_bg");
            let pr_notes_program = ShaderProgram::create_from_files(gl.clone(), "./shaders/piano_roll_note");

            // -------- PIANO ROLL BAR --------

            let pr_vertex_buffer = Buffer::new(gl.clone(), glow::ARRAY_BUFFER);
            pr_vertex_buffer.set_data(&QUAD_VERTICES, glow::STATIC_DRAW);

            let pr_index_buffer = Buffer::new(gl.clone(), glow::ELEMENT_ARRAY_BUFFER);
            pr_index_buffer.set_data(&QUAD_INDICES, glow::STATIC_DRAW);

            let pr_vertex_array = VertexArray::new(gl.clone());
            let pos_attrib = pr_program.get_attrib_location("vPos").unwrap();
            set_attribute!(glow::FLOAT, pr_vertex_array, pos_attrib, Vertex::0);

            let pr_instance_buffer = Buffer::new(gl.clone(), glow::ARRAY_BUFFER);
            let pr_bars_render = [
                RenderPianoRollBar {
                    0: 0.0,
                    1: 1.0,
                    2: 0
                }; 32
            ];
            pr_instance_buffer.set_data(pr_bars_render.as_slice(), glow::DYNAMIC_DRAW);

            let pr_bar_start = pr_program.get_attrib_location("barStart").unwrap();
            set_attribute!(glow::FLOAT, pr_vertex_array, pr_bar_start, RenderPianoRollBar::0);
            let pr_bar_length = pr_program.get_attrib_location("barLength").unwrap();
            set_attribute!(glow::FLOAT, pr_vertex_array, pr_bar_length, RenderPianoRollBar::1);
            let pr_bar_number = pr_program.get_attrib_location("barNumber").unwrap();
            set_attribute!(glow::UNSIGNED_INT, pr_vertex_array, pr_bar_number, RenderPianoRollBar::2);

            gl.vertex_attrib_divisor(1, 1);
            gl.vertex_attrib_divisor(2, 1);
            gl.vertex_attrib_divisor(3, 1);

            // -------- PIANO ROLL NOTES --------

            let pr_notes_vbo = Buffer::new(gl.clone(), glow::ARRAY_BUFFER);
            pr_notes_vbo.set_data(&QUAD_VERTICES, glow::STATIC_DRAW);

            let pr_notes_ebo = Buffer::new(gl.clone(), glow::ELEMENT_ARRAY_BUFFER);
            pr_notes_ebo.set_data(&QUAD_INDICES, glow::STATIC_DRAW);

            let pr_notes_vao = VertexArray::new(gl.clone());
            // let pos_attrib = pr_notes_program.get_attrib_location("vPos").unwrap();
            set_attribute!(glow::FLOAT, pr_notes_vao, 0, Vertex::0);

            let pr_notes_ibo = Buffer::new(gl.clone(), glow::ARRAY_BUFFER);
            let pr_notes_render = [
                RenderPianoRollNote {
                    0: [0.0, 1.0, 0.0, 1.0],
                    1: [1.0, 0.0, 0.0]
                }; NOTE_BUFFER_SIZE
            ];
            pr_notes_ibo.set_data(pr_notes_render.as_slice(), glow::DYNAMIC_DRAW);

            let pr_note_rect = pr_notes_program.get_attrib_location("noteRect").unwrap();
            set_attribute!(glow::FLOAT, pr_notes_vao, pr_note_rect, RenderPianoRollNote::0);
            let pr_note_color = pr_notes_program.get_attrib_location("noteColor").unwrap();
            set_attribute!(glow::FLOAT, pr_notes_vao, pr_note_color, RenderPianoRollNote::1);

            gl.vertex_attrib_divisor(1, 1);
            gl.vertex_attrib_divisor(2, 1);

            Self {
                navigation: nav,
                window_size: Vec2::new(0.0, 0.0),
                pr_program,
                pr_vertex_buffer,
                pr_vertex_array,
                pr_instance_buffer,
                pr_index_buffer,

                pr_notes_program,
                pr_notes_vao,
                pr_notes_vbo,
                pr_notes_ebo,
                pr_notes_ibo,

                gl,

                bars_render: pr_bars_render.to_vec(),
                notes_render: pr_notes_render.to_vec(),
                render_notes: HashMap::new(),

                ppq: 1920,
                note_colors: vec![
                    [1.0, 0.0, 0.0],
                    [1.0, 0.5, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 1.0, 1.0],
                    [0.0, 0.5, 1.0],
                    [0.0, 0.0, 1.0],
                    [0.5, 0.0, 1.0],
                    [1.0, 0.0, 1.0]
                ],

                last_note_start: 0,
                first_unhit_note: 0
            }
        }
    }
}

impl Renderer for PianoRollRenderer {
    fn draw(&mut self) {
        unsafe {
            // RENDER BARS

            let nav = self.navigation.lock().unwrap();

            {
                self.gl.use_program(Some(self.pr_program.program));
                //self.pr_vertex_array.bind();

                let mut curr_bar_tick = 0.0;
                let mut bar_id = 0;
                let mut bar_num = 0;
                {
                    let key_start = nav.key_pos;
                    let key_end = nav.key_pos + nav.zoom_keys;

                    self.pr_program.set_float("prBarBottom", (-key_start / (key_end - key_start)));
                    self.pr_program.set_float("prBarTop", ((128.0 - key_start) / (key_end - key_start)));
                    self.pr_program.set_float("width", self.window_size.x);
                    self.pr_program.set_float("height", self.window_size.y);

                    while curr_bar_tick < nav.zoom_ticks + nav.tick_pos {
                        bar_num += 1;
                        if (bar_num as f32) * ((self.ppq as f32) * 4.0) < nav.tick_pos {
                            curr_bar_tick += self.ppq as f32 * 4.0;
                            continue;
                        }
                        self.bars_render[bar_id] = RenderPianoRollBar {
                            0: ((curr_bar_tick - nav.tick_pos) / nav.zoom_ticks),
                            1: ((self.ppq as f32 * 4.0) / nav.zoom_ticks),
                            2: bar_num as u32 - 1
                        };
                        bar_id += 1;
                        if bar_id >= 32 {
                            self.pr_vertex_array.bind();
                            self.pr_instance_buffer.bind();
                            self.pr_vertex_buffer.bind();
                            self.pr_index_buffer.bind();
                            self.pr_instance_buffer.set_data(self.bars_render.as_slice(), glow::DYNAMIC_DRAW);
                            self.gl.draw_elements_instanced(
                                glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0, 32);
                            bar_id = 0;
                        }
                        curr_bar_tick += self.ppq as f32 * 4.0;
                    }
                }

                if bar_id != 0 {
                    self.pr_vertex_array.bind();
                    self.pr_instance_buffer.bind();
                    self.pr_vertex_buffer.bind();
                    self.pr_index_buffer.bind();
                    self.pr_instance_buffer.set_data(self.bars_render.as_slice(), glow::DYNAMIC_DRAW);

                    self.gl.use_program(Some(self.pr_program.program));
                    self.gl.draw_elements_instanced(
                            glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0, bar_id as i32);
                }

                self.gl.use_program(None);
            }

            // RENDER NOTES
            {
                self.gl.use_program(Some(self.pr_notes_program.program));

                {
                    self.pr_notes_program.set_float("width", self.window_size.x);
                    self.pr_notes_program.set_float("height", self.window_size.y);

                    let mut curr_time = 0.0;
                    let mut curr_note = 0;
                    let mut note_id = 0;
                    if self.render_notes.len() > 0 {
                        let notes = self.render_notes.get(&0).unwrap();

                        let note_start = {
                            let mut s = self.last_note_start;
                            for i in s..notes.len() {
                                if (notes[i].start + notes[i].length) as f32 > nav.tick_pos { break; }
                                s += 1;
                            }
                            self.last_note_start = s;
                            s
                        };

                        let note_end = {
                            let mut e = note_start;
                            for i in note_start..notes.len() {
                                if notes[i].start as f32 > nav.tick_pos + nav.zoom_ticks { break; }
                                e += 1;
                            }
                            e
                        };

                        for note in &notes[note_start..note_end]  {
                            {
                                /*if note.start + note.length < nav.tick_pos as u32 { 
                                    curr_note += 1; 
                                    if curr_note >= notes.len() {
                                        break;
                                    }
                                    continue;
                                }
                                if note.start > (nav.tick_pos + nav.zoom_ticks) as u32 {
                                    curr_note += 1; 
                                    if curr_note >= notes.len() {
                                        break;
                                    }
                                    break;
                                }*/

                                let note_bottom = (note.key as f32 - nav.key_pos) / (nav.zoom_keys);
                                let note_top = ((note.key as f32 + 1.0) - nav.key_pos) / (nav.zoom_keys);
                                self.notes_render[note_id] = RenderPianoRollNote {
                                    0: [(note.start as f32 - nav.tick_pos) / nav.zoom_ticks,
                                        (note.length as f32) / nav.zoom_ticks,
                                        (note_bottom),
                                        (note_top)],
                                    1: self.note_colors[(note.channel_track & 0xFF) as usize % self.note_colors.len()]
                                };
                                note_id += 1;
                                if note_id >= NOTE_BUFFER_SIZE {
                                    self.pr_notes_vao.bind();
                                    self.pr_notes_ibo.bind();
                                    self.pr_notes_vbo.bind();
                                    self.pr_notes_ebo.bind();
                                    self.pr_notes_ibo.set_data(self.notes_render.as_slice(), glow::DYNAMIC_DRAW);

                                    self.gl.use_program(Some(self.pr_notes_program.program));
                                    self.gl.draw_elements_instanced(
                                        glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0, NOTE_BUFFER_SIZE as i32);
                                    note_id = 0;
                                }
                                curr_note += 1;
                                if curr_note >= notes.len() {
                                    break;
                                }
                            }
                        }

                        if note_id != 0 {
                            self.pr_notes_vao.bind();
                            self.pr_notes_ibo.bind();
                            self.pr_notes_vbo.bind();
                            self.pr_notes_ebo.bind();
                            self.pr_notes_ibo.set_data(self.notes_render.as_slice(), glow::DYNAMIC_DRAW);

                            self.gl.use_program(Some(self.pr_notes_program.program));
                            self.gl.draw_elements_instanced(
                                glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0, note_id as i32);
                        }
                    }
                }

                self.gl.use_program(None);
            }
        }
    }

    fn window_size(&mut self, size: Vec2) {
        self.window_size = size;
    }

    fn update_ppq(&mut self, ppq: u16) {
        self.ppq = ppq;
    }

    fn update_project_notes(&mut self, project_notes: HashMap<usize, Vec<Arc<ProjectNote>>>) {
        self.render_notes = project_notes;
    }

    fn time_changed(&mut self, time: f32) {
        self.last_note_start = 0;
        self.first_unhit_note = 0;
    }
}