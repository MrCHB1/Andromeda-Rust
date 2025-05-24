use std::{path::{Path, PathBuf}, str::FromStr, sync::{atomic::{AtomicUsize, Ordering}, Arc, Mutex}, thread::JoinHandle, time::Duration};
use rand::Rng;

use cpal::{traits::{DeviceTrait, HostTrait}, BufferSize, Device, StreamConfig};
use xsynth_core::{channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ChannelInitOptions}, channel_group::{ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthEvent, SynthFormat, ThreadCount}, soundfont::{EnvelopeCurveType, EnvelopeOptions, Interpolator, SampleSoundfont, SoundfontBase, SoundfontInitOptions}, AudioPipe, AudioStreamParams, ChannelCount};

use std::sync::atomic::AtomicBool;
use crate::{audio, midi::events::{MIDIEvent, MIDIEventType}};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Realtime,
    Rendering
}

pub struct Limiter {
    loudness_l: f32,
    loudness_r: f32,
    velocity_r: f32,
    velocity_l: f32,
    pub attack: f32,
    pub falloff: f32,
    strength: f32,
    min_thresh: f32,
}

impl Limiter {
    pub fn new(attack: f32, release: f32, sample_rate: f32) -> Self {
        Self {
            loudness_l: 1.0,
            loudness_r: 1.0,
            velocity_l: 0.0,
            velocity_r: 0.0,
            attack: attack * sample_rate,
            falloff: release * sample_rate,
            strength: 1.0,
            min_thresh: 0.4,
        }
    }

    /// applies a filter to prevent audio clipping above 1 dB. 
    /// * `buffer` - the slice of the samples to apply the filter to
    pub fn apply_limiter(&mut self, buffer: &mut [f32]) -> () {
        let count = buffer.len();
        for i in (0..count).step_by(2) {
            let mut l = buffer[i].abs();
            let mut r = buffer[i+1].abs();

            if self.loudness_l > l {
                self.loudness_l = (self.loudness_l * self.falloff + l) / (self.falloff + 1.0);
            } else {
                self.loudness_l = (self.loudness_l * self.attack + l) / (self.attack + 1.0);
            }

            if self.loudness_r > r {
                self.loudness_r = (self.loudness_r * self.falloff + r) / (self.falloff + 1.0);
            } else {
                self.loudness_r = (self.loudness_r * self.attack + r) / (self.attack + 1.0);
            }

            if self.loudness_l < self.min_thresh { self.loudness_l = self.min_thresh; }
            if self.loudness_r < self.min_thresh { self.loudness_r = self.min_thresh; }

            l = buffer[i] / (self.loudness_l * self.strength + 2.0 * (1.0 - self.strength)) / 2.0;
            r = buffer[i + 1] / (self.loudness_r * self.strength + 2.0 * (1.0 - self.strength)) / 2.0;

            if i != 0 {
                let dl = (buffer[i] - l).abs();
                let dr = (buffer[i+1] - r).abs();

                if self.velocity_l > dl {
                    self.velocity_l = (self.velocity_l * self.falloff + dl) / (self.falloff + 1.0);
                } else {
                    self.velocity_l = (self.velocity_l * self.attack + dl) / (self.attack + 1.0);
                }

                if self.velocity_r > dr {
                    self.velocity_r = (self.velocity_r * self.falloff + dr) / (self.falloff + 1.0);
                } else {
                    self.velocity_r = (self.velocity_r * self.attack + dr) / (self.attack + 1.0);
                }
            }

            buffer[i] = l;
            buffer[i+1] = r;
        }
    }
}

struct PrerenderBuffer {
    pub read_pos: AtomicUsize,
    pub write_pos: AtomicUsize,
    pub audio_buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
}

impl PrerenderBuffer {
    pub fn new(audio_buffer: Arc<Mutex<Vec<f32>>>, sample_rate: u32, buffer_length: f32) -> Self {
        {
            let mut buffer = audio_buffer.lock().unwrap();
            *buffer = vec![0.0; (buffer_length * sample_rate as f32) as usize * 2]
        }
        Self {
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
            audio_buffer: audio_buffer,
            sample_rate
        }
    }

    /// Writes to the audio buffer, wrapping back to the beginning if start + count exceeds the buffer length.
    pub fn write_wrapped(&self, xsynth: &mut ChannelGroup, start: usize, count: usize) {
        {
            let mut audio_buffer = self.audio_buffer.lock().unwrap();
            let buff_len = audio_buffer.len();
            let start = (start * 2) % buff_len; 
            let mut count = count * 2;
            if start + count > buff_len {
                xsynth.read_samples(&mut audio_buffer[start..buff_len]);
                count -= buff_len - start;
                xsynth.read_samples(&mut audio_buffer[0..count]);
            } else {
                xsynth.read_samples(&mut audio_buffer[start..start+count]);
            }
        }
    }

    /// The function to render raw audio samples to the audio buffer.
    pub fn generator_func(self: Arc<Self>, xsynth: Arc<Mutex<ChannelGroup>>, events: Vec<MIDIEvent>, reset_flag: Arc<AtomicBool>) {
        self.write_pos.store(0, Ordering::SeqCst);
        self.read_pos.store(0, Ordering::SeqCst);

        let mut xsynth = xsynth.lock().unwrap();

        let buf_len = {
            let v = self.audio_buffer.lock().unwrap();
            v.len()
        };

        for e in events {
            std::thread::sleep(Duration::from_millis(2));
            if reset_flag.load(Ordering::SeqCst) { break; }

            let offset_samples = 
                (e.time * self.sample_rate as f32) as isize -  self.write_pos.load(Ordering::SeqCst) as isize;
            
            if offset_samples > 0 {
                let mut remaining = offset_samples as usize;
                while self.write_pos.load(Ordering::SeqCst) + remaining > self.read_pos.load(Ordering::SeqCst) + buf_len / 2 {
                    let mut spare = (self.read_pos.load(Ordering::SeqCst) + buf_len / 2) as isize - self.write_pos.load(Ordering::SeqCst) as isize;
                    if spare > 0 {
                        if spare > remaining as isize {
                            spare = remaining as isize;
                        }
                        if spare != 0 {
                            let spare = spare as usize;
                            self.write_wrapped(&mut xsynth, self.write_pos.load(Ordering::SeqCst), spare);
                            self.write_pos.fetch_add(spare, Ordering::SeqCst);
                            remaining -= spare;
                        }
                        if remaining == 0 { break; }
                    }
                    if reset_flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
                if remaining != 0 {
                    self.write_wrapped(&mut xsynth, self.write_pos.load(Ordering::SeqCst), remaining);
                }
                self.write_pos.fetch_add(remaining, Ordering::SeqCst);
            }

            /*if self.write_pos < self.read_pos.load(Ordering::SeqCst) {
                self.write_pos = self.read_pos.load(Ordering::SeqCst);
            }
            let ev_time = e.time;
            let offset = ev_time;
            let samples = (offset * self.sample_rate as f32) as isize - self.write_pos as isize;

            if samples > 0 {
                let mut samples = samples as usize;
                while self.write_pos + samples > self.read_pos.load(Ordering::SeqCst) + audio_buffer_len / 2 {
                    let mut spare = (self.read_pos.load(Ordering::SeqCst) + audio_buffer_len / 2) - self.write_pos;
                    if spare > 0 {
                        if spare > samples { spare = samples; }
                        if spare != 0 {
                            self.write_wrapped(&mut xsynth, self.write_pos, spare);
                            samples -= spare;
                            self.write_pos += spare;
                        }
                        if samples == 0 { break; }
                    }
                    std::thread::sleep(Duration::from_millis(1));
                    if *reset_requested.lock().unwrap() { break; }
                }
                if samples != 0 {
                    self.write_wrapped(&mut xsynth, self.write_pos, samples);
                }
                self.write_pos += samples;
            }*/

            match e.event_type {
                MIDIEventType::NoteOn => {
                    let vel = e.data[2];
                    if vel < self.get_skipping_velocity() { continue; }
                    (*xsynth).send_event(SynthEvent::Channel(
                        (e.data[0] & 0xF) as u32, ChannelEvent::Audio(
                            ChannelAudioEvent::NoteOn { key: e.data[1], vel: e.data[2] }
                        )));
                },
                MIDIEventType::NoteOff => {
                    (*xsynth).send_event(SynthEvent::Channel(
                        (e.data[0] & 0xF) as u32, ChannelEvent::Audio(
                            ChannelAudioEvent::NoteOff { key: e.data[1] }
                        )));
                }
            }
        }

        (*xsynth).send_event(SynthEvent::AllChannels(
            ChannelEvent::Audio(
                ChannelAudioEvent::AllNotesKilled
            )
        ));
    }

    pub fn get_skipping_velocity(&self) -> u8 {
        let wr = self.write_pos.load(Ordering::SeqCst);
        let rd = self.read_pos.load(Ordering::SeqCst);
        let mut diff = 127 + 10 - (wr as i32 - rd as i32) / 100;
        if diff > 127 { diff = 127; }
        if diff < 0 { diff = 0; }
        diff as u8
    }
}

pub struct PrerenderedAudio {
    pub render_mode: Arc<Mutex<RenderMode>>,
    audio_buffer: Arc<PrerenderBuffer>,

    xsynth: Arc<Mutex<ChannelGroup>>,
    stream_params: AudioStreamParams,
    pub events: Arc<Mutex<Vec<MIDIEvent>>>,
    device: Device,
    cfg: StreamConfig,

    generator_thread: Option<JoinHandle<()>>,
    reset_requested: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    limiter: Arc<Mutex<Limiter>>
}

impl PrerenderedAudio {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let cfg = device.default_output_config().unwrap();
        let mut cfg: StreamConfig = cfg.into();
        cfg.buffer_size = BufferSize::Fixed(1024);

        let sr = cfg.sample_rate.0;
        let stream_params = AudioStreamParams::new(cfg.sample_rate.0, ChannelCount::Stereo);
        let buffer = Arc::new(Mutex::new(Vec::new()));

        let s = Self {
            render_mode: Arc::new(Mutex::new(RenderMode::Realtime)),
            audio_buffer: Arc::new(
                PrerenderBuffer::new(buffer.clone(), sr, 60.0)
            ),
            xsynth: Arc::new(Mutex::new(ChannelGroup::new(
                ChannelGroupConfig {
                    channel_init_options: ChannelInitOptions {
                        fade_out_killing: false
                    },
                    format: SynthFormat::Midi,
                    audio_params: stream_params,
                    parallelism: ParallelismOptions {
                        channel: ThreadCount::Auto,
                        key: ThreadCount::None
                    }
                }
            ))),
            stream_params,
            device,
            cfg,
            events: Arc::new(Mutex::new(Vec::new())),

            generator_thread: None,
            reset_requested: Arc::new(AtomicBool::new(false)),
            buffer,
            limiter: Arc::new(Mutex::new(Limiter::new(0.01, 0.1, sr as f32)))
        };
        s
    }

    pub fn load_soundfonts(&mut self, sfs: &[String]) {
        let mut synth_soundfont: Vec<Arc<dyn SoundfontBase>> = Vec::new();
        for sf in sfs {
            synth_soundfont.push(Arc::new(
                SampleSoundfont::new(Path::new(sf), self.stream_params, SoundfontInitOptions {
                    bank: None,
                    preset: None,
                    vol_envelope_options: EnvelopeOptions {
                        attack_curve: EnvelopeCurveType::Linear,
                        decay_curve: EnvelopeCurveType::Linear,
                        release_curve: EnvelopeCurveType::Linear
                    },
                    use_effects: false,
                    interpolator: Interpolator::Linear
                }).unwrap()
            ))
        }

        if let Ok(mut xsynth) = self.xsynth.lock() {
            xsynth.send_event(
                SynthEvent::AllChannels(
                    ChannelEvent::Config(
                        ChannelConfigEvent::SetSoundfonts(
                            synth_soundfont.clone()
                        )
                    )
                )
            );
        }
    }

    /// Sets the MIDI events for the Prerenderer to loop through when rendering. Ineffective if `[events]` has a length of zero.
    pub fn set_events(&mut self, events: Vec<MIDIEvent>) {
        if events.len() > 0 {
            *self.events.lock().unwrap() = events;
        }
    }

    pub fn set_layer_count(&mut self, layer_count: usize) {
        if let Ok(mut xsynth) = self.xsynth.lock() {
            xsynth.send_event(
                SynthEvent::AllChannels(
                    ChannelEvent::Config(
                        ChannelConfigEvent::SetLayerCount(
                            Some(layer_count)
                        )
                    )
                )
            );
        }
    }

    pub fn note_on(&mut self, channel: u32, key: u8, velocity: u8) {
        if let Ok(mut xsynth) = self.xsynth.lock() {
            xsynth.send_event(
                SynthEvent::Channel(channel, 
                    ChannelEvent::Audio(
                        ChannelAudioEvent::NoteOn { key: key, vel: velocity }
                    )
                )
            );
        }
    }

    pub fn note_off(&mut self, channel: u32, key: u8) {
        if let Ok(mut xsynth) = self.xsynth.lock() {
            xsynth.send_event(
                SynthEvent::Channel(channel, 
                    ChannelEvent::Audio(
                        ChannelAudioEvent::NoteOff { key }
                    )
                )
            );
        }
    }


    pub fn build_stream(&mut self) -> cpal::Stream {
        let xs = self.xsynth.clone();
        let rm = self.render_mode.clone();
        let rr = self.reset_requested.clone();
        let lim = self.limiter.clone();

        let audio_buffer = Arc::clone(&self.audio_buffer);
        let buffer = self.buffer.clone();

        self.device.build_output_stream(&self.cfg, move |data: &mut [f32], _| {
            let mode = *rm.lock().unwrap();
            match mode {
                RenderMode::Realtime => {
                    xs.lock().unwrap()
                        .read_samples(data);
                },
                RenderMode::Rendering => {
                    let count = data.len();
                    if rr.load(Ordering::SeqCst) {
                        data.fill(0.0);
                        return;
                    }

                    //let rp = audio_buffer.read_pos.load(Ordering::SeqCst);
                    //let wp = audio_buffer.write_pos.load(Ordering::SeqCst);
                    let read = { 
                        let buf = buffer.lock().unwrap();
                        audio_buffer.read_pos.load(Ordering::SeqCst) % (buf.len() / 2)
                    };
                    if audio_buffer.read_pos.load(Ordering::SeqCst) + count / 2 > audio_buffer.write_pos.load(Ordering::SeqCst) {
                        let mut copy_count = audio_buffer.read_pos.load(Ordering::SeqCst) as isize - (audio_buffer.write_pos.load(Ordering::SeqCst) + count / 2) as isize;
                        if copy_count > count as isize / 2 {
                            copy_count = count as isize / 2;
                        }
                        if copy_count > 0 {
                            let buf = buffer.lock().unwrap();
                            for i in 0..(copy_count * 2) {
                                let i = i as usize;
                                data[i] = buf[(i + read * 2) % buf.len()];
                            }
                        } else {
                            copy_count = 0;
                        }
                        for i in (copy_count * 2)..(count as isize) {
                            data[i as usize] = 0.0;
                        }
                    } else {
                        let buf = buffer.lock().unwrap();
                        for i in 0..count {
                            data[i] = buf[(i + read * 2) % buf.len()];
                        }
                    }

                    audio_buffer.read_pos
                        .fetch_add(data.len() / 2, Ordering::SeqCst);
                }
            }
            lim.lock().unwrap().apply_limiter(data);
        }, |err| {
            println!("{}", err.to_string());
        }, None).unwrap()
    }

    pub fn start_render_thread(&mut self) -> std::thread::JoinHandle<()> {
        let pr = self.audio_buffer.clone();
        let xsynth = self.xsynth.clone();
        let evs = std::mem::take(&mut *self.events.lock().unwrap());

        let rr = self.reset_requested.clone();

        std::thread::spawn(move || {
            //audio_buffer.lock().unwrap().generator_func(xsynth, evs, rr);
            pr.generator_func(xsynth, evs, rr);
        })
    }

    fn kill_last_generator(&mut self) {
        self.reset_requested.store(true, Ordering::SeqCst);
        if let Some(thread) = self.generator_thread.take() {
            thread.join().unwrap();
        }
    }

    pub fn start(&mut self) {
        self.kill_last_generator();
        self.reset_requested.store(false, Ordering::SeqCst);
        self.generator_thread = Some(self.start_render_thread());
    }

    pub fn stop(&mut self) {
        self.kill_last_generator();
        self.reset_requested.store(true, Ordering::SeqCst);
        self.generator_thread = None;
        self.audio_buffer.read_pos.store(0, Ordering::SeqCst);
        self.audio_buffer.write_pos.store(0, Ordering::SeqCst);
    }

    /// If using `[RenderMode::Realtime]`, then prerendering is not done. Otherwise, the prerenderer starts the renderthread immediately.
    pub fn switch_render_mode(&mut self, rm: RenderMode) {
        match rm {
            RenderMode::Realtime => {
                self.stop();
            },
            RenderMode::Rendering => {
                self.start();
            }
        }

        {
            let mut render_mode = self.render_mode.lock().unwrap();
            *render_mode = rm;
        }
    }
}