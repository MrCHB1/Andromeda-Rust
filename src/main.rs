use audio::{playback::Playback, prerenderer::{PrerenderedAudio, RenderMode}};
use cpal::{traits::StreamTrait, Stream};
use editor::{navigation::Navigation, project_settings::ProjectSettings, settings::ApplicationSettings};
use eframe::{egui::{self, vec2, Color32, Event, EventFilter, Key, Layout, PointerButton, RichText, Style, Ui}, egui_glow::CallbackFn, glow};
use eframe::glow::HasContext;
use midi::{events::{MIDIEvent, TempoEvent}, io::midi_file::MIDIFile, notes::{Note, ProjectNoteManager}};
use rendering::piano_roll::{PianoRollRenderer, Renderer};
use std::{ops::DerefMut, path::absolute, process::exit};
use std::sync::{Arc, Mutex};
use sysinfo::System;

mod rendering;
mod editor;
mod audio;
mod midi;

#[derive(PartialEq, Eq)]
enum CurrentAppSettings {
    None,
    General,
    Audio
}

impl Default for CurrentAppSettings {
    fn default() -> Self {
        CurrentAppSettings::None
    }
}

#[derive(Default)]
struct MainWindow {
    sys: System,
    gl: Option<Arc<glow::Context>>,
    renderer: Option<Arc<Mutex<dyn Renderer + Send + Sync>>>,
    nav: Option<Arc<Mutex<Navigation>>>,

    window_settings: CurrentAppSettings,
    app_settings: Arc<Mutex<ApplicationSettings>>,
    project_settings: ProjectSettings,
    synth: Option<PrerenderedAudio>,

    synth_init: bool,
    curr_pointer_key: u8,
    note_playing: bool,
    stream: Option<Stream>,
    playback: Playback,
    last_tick: f32,

    project_note_manager: ProjectNoteManager
}

impl MainWindow {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut synth = PrerenderedAudio::new();
        synth.load_soundfonts(&[
            "./assets/soundfonts/Sinufont.sf2".to_string()
        ]);
        synth.set_layer_count(2);

        let mut s = Self::default();
        let initial_tempo = s.project_settings.initial_bpm;
        let initial_tempo_raw = (60000000.0 / initial_tempo) as u32;

        s.playback.tempo_events.push(TempoEvent {
            time: 0,
            time_norm: 0.0,
            tempo: initial_tempo
        });

        s.stream = Some(synth.build_stream());
        s.synth = Some(synth);
        s
    }

    fn init_gl(&mut self) {
        let gl = self.gl.as_ref().unwrap();

        let nav = Arc::new(Mutex::new(Navigation::new()));
        let mut renderer = PianoRollRenderer::new(nav.clone(), gl.clone());
        renderer.update_ppq(self.project_settings.ppq);
        self.nav = Some(nav);
        self.renderer = Some(Arc::new(Mutex::new(renderer)));
    }

    fn labeled_widget<R>(&mut self, label: &str, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:",label)).size(15.0));
            ui.horizontal(contents);
        });
    }

    /// Handles the zooming and panning across the piano roll / track view.
    /// `[scroll_delta]` - the amount that the user scrolled
    /// `[is_moving]` - if wheel scroll should move the piano roll instead of zooming
    /// `[vertical_zoom]` - if the user should zoom on the keys (vertical axis) instead
    fn handle_navigation(&mut self, ctx: &egui::Context, ui: &mut Ui, is_moving: bool, vertical_zoom: bool) {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta).y;
        if (scroll_delta.abs() > 0.001) {
            let mut nav = self.nav.as_mut().unwrap();
            let mut nav = nav.lock().unwrap();
            // yanderedev ahh statements 💀
            if is_moving {
                let move_by = scroll_delta;
                if vertical_zoom {
                    let mut new_key_pos = nav.key_pos + move_by * (nav.zoom_keys / 128.0);
                    if new_key_pos < 0.0 { new_key_pos = 0.0; }
                    if new_key_pos + nav.zoom_keys > 128.0 { new_key_pos = 128.0 - nav.zoom_keys; }

                    nav.key_pos = new_key_pos;
                } else {
                    let mut new_tick_pos = nav.tick_pos + move_by * (nav.zoom_ticks / self.project_settings.ppq as f32);
                    if new_tick_pos < 0.0 { new_tick_pos = 0.0; }

                    let rend = self.renderer.as_mut().unwrap();
                    nav.change_tick_pos(new_tick_pos, |time| rend.lock().unwrap().time_changed(time));
                } 
            } else {
                let zoom_factor = 1.01f32.powf(scroll_delta);
                // vertical zoom
                if vertical_zoom { 
                    let view_top = nav.key_pos + nav.zoom_keys;

                    nav.zoom_keys *= zoom_factor;
                    if nav.zoom_keys < 12.0 {
                        nav.zoom_keys = 12.0;
                    }
                    if nav.zoom_keys > 128.0 {
                        nav.zoom_keys = 128.0;
                    }

                    let view_top_new = nav.key_pos + nav.zoom_keys;
                    let view_top_delta = view_top_new - view_top;
                    if view_top_new > 128.0 { nav.key_pos -= view_top_delta; }

                    // clamp key view
                    if nav.key_pos < 0.0 { nav.key_pos = 0.0; }
                } else { 
                    // horizontal zoom
                    nav.zoom_ticks *= zoom_factor;
                    if nav.zoom_ticks < 10.0 {
                        nav.zoom_ticks = 10.0;
                    }
                    if nav.zoom_ticks > 384000.0 {
                        nav.zoom_ticks = 384000.0;
                    }
                }
            }
        }
    }
}

impl eframe::App for MainWindow {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(mut synth) = self.synth.as_mut() {
            if !self.synth_init {
                self.stream.as_mut().unwrap().play().unwrap();
                self.synth_init = true;
            }
        }

        if self.gl.is_none() {
            if let Some(gl) = frame.gl() {
                self.gl = Some(gl.clone());
                self.init_gl();
            }
        }

        let mut hover_info = "";

        if self.playback.is_playing {
            if let Some(nav) = self.nav.as_ref() {
                let mut nav = nav.lock().unwrap();
                nav.tick_pos = self.playback.get_playback_time(self.project_settings.ppq);
                ctx.request_repaint();
            }
        }

        if self.project_note_manager.render_needs_update {
            if let Some(renderer) = self.renderer.as_mut() {
                let notes = self.project_note_manager.get_notes();
                {
                    //let mut renderer = renderer.lock().unwrap();
                    renderer.lock().unwrap().update_project_notes(notes);
                }
                self.project_note_manager.render_needs_update = false;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {

            hover_info = "";

            let mut sys = &mut self.sys;
            sys.refresh_cpu_usage();

            egui::TopBottomPanel::top("menu_bar")
                .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.image(egui::include_image!("../assets/Andromeda_Logo.png"));
                    ui.menu_button("File", |ui| {
                        if ui.button("Import MIDI file").clicked() {
                            let midi_fd = rfd::FileDialog::new()
                                .add_filter("MIDI Files", &["mid","midi"]);
                            if let Some(file) = midi_fd.pick_file() {
                                let midi = MIDIFile::new(String::from(file.to_str().unwrap()), true)
                                    .unwrap();

                                self.project_settings.ppq = midi.ppq;

                                let mut midi_evs = Vec::new();
                                let mut notes = Vec::new();
                                let mut tempo_evs = Vec::new();
                                midi.get_sequences(&mut midi_evs, &mut notes, &mut tempo_evs);

                                if let Some(synth) = self.synth.as_mut() {
                                    synth.set_events(midi_evs);
                                    // println!("{:?}", synth.events);
                                }

                                self.playback.tempo_events = tempo_evs;

                                for note_key in notes {
                                    self.project_note_manager.convert_notes(note_key);
                                }
                                self.project_note_manager.render_needs_update = true;
                            }
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        
                    });
                    ui.menu_button("Options", |ui| {
                        if ui.button("Audio...").clicked() {
                            self.window_settings = CurrentAppSettings::Audio;
                        }
                    });
                    ui.menu_button("Project", |ui| {
                        if ui.button("Close project").clicked() {
                            exit(0); 
                        }
                    });
                    ui.menu_button("Tools", |ui| {
                        
                    });
                    ui.menu_button("Help", |ui| {
                        
                    });
                });
            });

            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.label(format!("CPU {:.1}%", sys.cpus()[0].cpu_usage())).hovered() {
                        hover_info = "Your CPU's usage.";
                    }
                    ui.label(format!("{}", hover_info));
                })
            });

            egui::SidePanel::new(egui::panel::Side::Right, "thing")
                .resizable(false)
                .default_width(30f32)
                .show(ctx, |ui| {
                    ui.button("copy");
                    ui.button("paste");
                    ui.button("cut");
            });

            egui::CentralPanel::default()
                .show(ctx, |ui| {
                    let available_size = ui.available_size_before_wrap();
                    let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());

                    if self.gl.is_none() { return; }
                    if self.renderer.is_none() { return; }
                    if self.nav.is_none() { return; }

                    let (alt_down, shift_down, ctrl_down)  =   ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.ctrl) );

                    self.handle_navigation(ctx, ui, ctrl_down, alt_down);

                    if let Some(synth) = self.synth.as_mut() {
                        if !self.playback.is_playing { 
                            if ui.input(|i| i.pointer.button_down(PointerButton::Primary)) {
                                if (self.nav.is_none()) { return; }
                                let pos = ui.input(|i| i.pointer.interact_pos()).unwrap();
                                let nav = self.nav.as_ref().unwrap();
                                {
                                    let nav = nav.lock().unwrap();
                                    let curr_key = ((1.0 - (pos.y - rect.y_range().min) / available_size.y) * nav.zoom_keys + nav.key_pos) as u8;
                                    if curr_key != self.curr_pointer_key || !self.note_playing {
                                        synth.note_off(0, self.curr_pointer_key);
                                        synth.note_on(0, curr_key, 127);
                                        self.note_playing = true;
                                    }
                                    self.curr_pointer_key = curr_key;
                                }
                            }
                            if ui.input(|i| i.pointer.primary_released()) {
                                synth.note_off(0, self.curr_pointer_key);
                                self.note_playing = false;
                            }
                        }
                    }

                    if ui.input(|i| i.key_pressed(Key::Space)) {
                        self.playback.play_or_stop();
                        if let Some(nav) = self.nav.as_ref() {
                            let mut nav = nav.lock().unwrap();
                            if !self.playback.is_playing {
                                //nav.tick_pos = self.last_tick;
                                if let Some(rend) = self.renderer.as_mut() {
                                    let mut rend = rend.lock().unwrap();
                                    nav.change_tick_pos(self.last_tick, |time| { rend.time_changed(time) });
                                }
                            } else {
                                self.last_tick = nav.tick_pos;
                            }

                            if let Some(synth) = self.synth.as_mut() {
                                if !self.playback.is_playing {
                                    synth.switch_render_mode(RenderMode::Realtime);
                                } else {
                                    synth.switch_render_mode(RenderMode::Rendering);
                                }
                            }

                            ctx.request_repaint();
                        }
                    }
                    
                    let gl = self.gl.as_ref().unwrap();
                    let renderer = self.renderer.as_ref().unwrap();

                    let callback = egui::PaintCallback {
                        rect,
                        callback: Arc::new(CallbackFn::new({
                            let gl = Arc::clone(&gl);
                            let renderer = Arc::clone(&renderer);

                            move |_info, painter| {
                                unsafe {
                                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                                    gl.clear(glow::COLOR_BUFFER_BIT);
                                    {
                                        let mut rnd = renderer.lock().unwrap();
                                        (*rnd).window_size(rect.size());
                                        (*rnd).draw();
                                    }
                                }
                            }
                        })),
                    };
                    ui.painter().add(callback);
                });
        });

        if self.window_settings != CurrentAppSettings::None {
            egui::Window::new("Settings")
                .collapsible(false)
                .resizable(false)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                if ui.selectable_label(self.window_settings == CurrentAppSettings::General, "General").clicked() {
                                    self.window_settings = CurrentAppSettings::General;
                                }
                                if ui.selectable_label(self.window_settings == CurrentAppSettings::Audio, "Audio").clicked() {
                                    self.window_settings = CurrentAppSettings::Audio;
                                }
                            });
                            ui.separator();
                            ui.vertical(|ui| {
                                match self.window_settings {
                                    CurrentAppSettings::General => {
                                        
                                    },
                                    CurrentAppSettings::Audio => {
                                        ui.vertical(|ui| {
                                            let app_settings = self.app_settings.clone();
                                            let mut app_settings = app_settings.lock().unwrap();

                                            self.labeled_widget("Soundfont", ui, |ui| {
                                                ui.label(format!("{}", app_settings.audio_settings.soundfont_path));
                                                if ui.button("Choose soundfont").clicked() {
                                                    let sfd = rfd::FileDialog::new()
                                                        .add_filter("Soundfont Files", &["sfz","sf2"]);
                                                    if let Some(file) = sfd.pick_file() {
                                                        let path = file.to_string_lossy().to_string();
                                                        app_settings.audio_settings.soundfont_path = path;
                                                    }
                                                }
                                            });

                                            self.labeled_widget("Layers", ui, |ui| {
                                                ui.add(egui::DragValue::new(&mut app_settings.audio_settings.num_layers).range(1..=10));
                                            });
                                            /*ui.vertical(|ui| {
                                                ui.label(RichText::new("Soundfont").size(15.0));
                                                ui.horizontal(|ui| {
                                                    ui.label("None selected!");
                                                    if ui.button("Choose soundfont").clicked() {
                                                        let sfd = rfd::FileDialog::new()
                                                            .add_filter("Soundfont Files", &["*.sfz","*.sf2"]);
                                                        if let Some(file) = sfd.pick_file() {

                                                        }
                                                    }
                                                });
                                            });*/
                                        });
                                    },
                                    CurrentAppSettings::None => {

                                    }
                                }
                            });
                        });
                        ui.add_space(20.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0), 
                            Layout::bottom_up(egui::Align::LEFT), 
                            |ui| {
                                if ui.button("Cancel").clicked() {
                                    self.window_settings = CurrentAppSettings::None;
                                }
                            })
                    });
                /*egui::TopBottomPanel::bottom("settings_footer").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            self.window_settings_flag = -1;
                        }
                    });
                });*/
            });
        }
    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native("Andromeda", native_options, Box::new(|cc| {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Ok(Box::new(MainWindow::new(cc)))
    }))
}