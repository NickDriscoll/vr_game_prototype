extern crate minimp3 as mp3;

use alto::{sys::ALint, Source, SourceState, StaticSource};
use tfd::MessageBoxIcon;
use std::sync::Arc;
use std::collections::HashMap;
use std::fs::{File};
use std::io::{Seek, SeekFrom};
use std::sync::mpsc::{Receiver};
use std::thread;
use std::time::Duration;
use crate::structs::Configuration;

pub const DEFAULT_BGM_PATH: &str = "music/cryptic_relics.mp3";

const IDEAL_FRAMES_QUEUED: ALint = 5;   //Ideal number of queued audio frames for streaming sources

pub struct RequestedSoundEffect {
    pub id: Option<usize>,
    pub path: String,
    pub position: [f32; 3],
    pub linear_gain: f32,
    pub looping: bool
}

pub struct ActiveSoundEffect {
    pub id: Option<usize>,
    pub source: StaticSource    
}

//Represents the kinds of messages the audio system can receive from the main thread
pub enum AudioCommand {
    SetListenerPosition([f32; 3]),
    SetListenerVelocity([f32; 3]),
    SetListenerOrientation(([f32; 3], [f32; 3])),
    SetListenerGain(f32),
    SetPitchShift(f32),
    LoadSFX(String),
    PlaySFX(RequestedSoundEffect),
    StopSFX(usize),
    SelectNewBGM,
    RestartBGM,
    PlayPause
}

//Returns an mp3 decoder given a filepath
fn load_decoder(path: &str) -> Option<mp3::Decoder<File>> {
    match File::open(path) {
        Ok(f) => { 
            Some(mp3::Decoder::new(f))
        }
        Err(e) => {
            tfd::message_box_ok("Error loading mp3", &format!("Unable to open: {}\n{}", path, e), MessageBoxIcon::Error);
            None
        }
    }
}

//Gain is a non-linear quantity, so we do this conversion in order to translate from a volume value that is linear
fn linearize_gain(linear_gain: f32) -> f32 {
    (f32::exp(linear_gain / 100.0) - 1.0) / (glm::e::<f32>() - 1.0)
}

fn set_linearized_gain(ctxt: &alto::Context, linear_gain: f32) {
    ctxt.set_gain(linearize_gain(linear_gain)).unwrap();
}

//Main function for the audio system
pub fn audio_main(audio_receiver: Receiver<AudioCommand>, config: &Configuration) {
    //Allocation is necessary here because we are moving this into another thread
    let mut bgm_path = match config.string_options.get(Configuration::MUSIC_NAME) {
        Some(path) => { String::from(path) }
        None => { String::from(DEFAULT_BGM_PATH) }
    };

    let bgm_volume = match config.float_options.get(Configuration::BGM_VOLUME) {
        Some(v) => { *v }
        None => { 10.0 }
    };

    thread::spawn(move || {
        //Initializing the OpenAL context
        //This can fail if OpenAL is not installed on the host system
        let alto_context = match alto::Alto::load_default() {
            Ok(a) => { 
                let alto = a;
                match alto.default_output() {
                    Some(string) => {
                        match alto.open(Some(&string)) {
                            Ok(dev) => {
                                match dev.new_context(None) {
                                    Ok(ctxt) => { ctxt }
                                    Err(e) => {
                                        tfd::message_box_ok("OpenAL Error", &format!("Error creating OpenAL context: {}\n\nThe game will still work, but without any audio.", e), tfd::MessageBoxIcon::Warning);
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                tfd::message_box_ok("OpenAL Error", &format!("Error opening default audio device: {}\n\nThe game will still work, but without any audio.", e), tfd::MessageBoxIcon::Warning);
                                return;
                            }
                        }
                    }
                    None => {
                        tfd::message_box_ok("OpenAL Error", "No default audio output device found\n\nThe game will still work, but without any audio.", tfd::MessageBoxIcon::Warning);
                        return;
                    }
                }
            }
            Err(e) => {
                tfd::message_box_ok("OpenAL Error", &format!("Error initializing OpenAL: {}\n\nThe game will still work, but without any audio.", e), tfd::MessageBoxIcon::Warning);
                return;
            }
        };
        set_linearized_gain(&alto_context, bgm_volume);

        //Hashmap for assiciating sfx paths with their loaded audio data
        let mut sfx_buffers = HashMap::new();

        const STATIC_SOURCE_LIMIT: usize = 64;
        let mut active_sfx: Vec<ActiveSoundEffect> = Vec::with_capacity(STATIC_SOURCE_LIMIT);
        /*
        for i in 0..STATIC_SOURCE_LIMIT {
            let source = alto_context.new_static_source().unwrap();
            let sfx = ActiveSoundEffect {
                id: i,
                source
            };
            active_sfx.push(sfx);
        }
        */

        //Initialize the mp3 decoder with the default bgm
        let mut bgm_decoder = load_decoder(&bgm_path);
        let mut bgm_source = alto_context.new_streaming_source().unwrap();
        let mut start_bgm = true;

        loop {
            //Process all commands from the main thread
            while let Ok(command) = audio_receiver.try_recv() {
                match command {
                    AudioCommand::SetListenerPosition(pos) => { alto_context.set_position(pos).unwrap(); }
                    AudioCommand::SetListenerVelocity(vel) => { alto_context.set_velocity(vel).unwrap(); }
                    AudioCommand::SetListenerOrientation(ori) => { alto_context.set_orientation(ori).unwrap(); }
                    AudioCommand::SetListenerGain(volume) => { set_linearized_gain(&alto_context, volume); }
                    AudioCommand::SetPitchShift(shift) => { bgm_source.set_pitch(shift).unwrap(); }
                    AudioCommand::LoadSFX(path) => {
                        let mut freq = 0;
                        let mut samples = Vec::new();
                        if let Some(mut sfx_decoder) = load_decoder(&path) {
                            //Sound effects are assumed to be mono
                            loop {
                                match sfx_decoder.next_frame() {
                                    Ok(frame) => {
                                        freq = frame.sample_rate;
                                        for sample in frame.data {
                                            samples.push(
                                                alto::Mono {
                                                    center: sample
                                                }
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        match e {
                                            mp3::Error::Eof => {}
                                            _ => { println!("Error decoding mp3 frame: {}", e); }
                                        }
                                        break;
                                    }
                                }
                            }
                        }

                        let b = alto_context.new_buffer(samples, freq).unwrap();
                        sfx_buffers.insert(path, Arc::new(b));
                    }
                    AudioCommand::PlaySFX(sound_effect) => {
                        match sfx_buffers.get(&sound_effect.path) {
                            Some(buffer) => {
                                let mut available = false;
                                for sfx in &mut active_sfx {
                                    let source = &mut sfx.source;
                                    if source.state() != SourceState::Playing {
                                        sfx.id = sound_effect.id;
                                        source.set_position(sound_effect.position).unwrap();
                                        source.set_buffer(buffer.clone()).unwrap();
                                        source.set_gain(linearize_gain(sound_effect.linear_gain)).unwrap();
                                        source.set_looping(sound_effect.looping);
                                        source.play();
                                        available = true;
                                        break;
                                    }
                                }

                                if active_sfx.len() < STATIC_SOURCE_LIMIT {
                                    let mut source = alto_context.new_static_source().unwrap();
                                    source.set_position(sound_effect.position).unwrap();
                                    source.set_buffer(buffer.clone()).unwrap();
                                    source.set_gain(linearize_gain(sound_effect.linear_gain)).unwrap();
                                    source.play();
                                    available = true;
                                    let sfx = ActiveSoundEffect {
                                        id: sound_effect.id,
                                        source
                                    };
                                    active_sfx.push(sfx);
                                }

                                if !available {
                                    println!("No available sfx slot to play {}", sound_effect.path);
                                }
                            }
                            None => {
                                println!("{} hasn't been loaded yet", sound_effect.path);
                            }
                        }
                    }
                    AudioCommand::StopSFX(req_id) => {
                        for sfx in &mut active_sfx {
                            if let Some(id) = sfx.id {
                                if id == req_id {
                                    sfx.source.stop();
                                    break;
                                }
                            }
                        }
                    }
                    AudioCommand::SelectNewBGM => {
                        bgm_source.pause();
                        match tfd::open_file_dialog("Choose bgm", "music/", Some((&["*.mp3"], "mp3 files (*.mp3)"))) {
                            Some(path) => {
                                let pitch = bgm_source.pitch();
                                bgm_source.stop();
                                bgm_decoder = load_decoder(&path);
                                bgm_path = path;
                            
                                //Clear out any residual sound data from the old mp3
                                bgm_source = alto_context.new_streaming_source().unwrap();
                                bgm_source.set_pitch(pitch).unwrap();
                                start_bgm = true;
                            }
                            None => { bgm_source.play(); }
                        }
                    }
                    AudioCommand::RestartBGM => {
                        bgm_source.pause();
                        if let Some(_) = &mut bgm_decoder {
                            bgm_source.stop();
                            bgm_decoder = load_decoder(&bgm_path);
                            start_bgm = true;
                        }
                    }
                    AudioCommand::PlayPause => {
                        start_bgm = !start_bgm;
                        match bgm_source.state() {
                            SourceState::Playing | SourceState::Initial => {
                                bgm_source.pause();                                
                                start_bgm = false;
                            }
                            SourceState::Paused | SourceState::Stopped => {
                                bgm_source.play();
                                start_bgm = true;
                            }
                            SourceState::Unknown(code) => { println!("Source is in an unknown state: {}", code); }
                        }
                    }
                }
            }

            //If there are fewer than the ideal number of frames queued, prepare and queue a frame
            if bgm_source.buffers_queued() < IDEAL_FRAMES_QUEUED {
                if let Some(decoder) = &mut bgm_decoder {
                    match decoder.next_frame() {
                        Ok(frame) => {
                            if frame.channels == 1 {            //Mono
                                let mut mono_samples = Vec::with_capacity(frame.data.len());
                                for sample in frame.data {
                                    mono_samples.push(
                                        alto::Mono {
                                            center: sample
                                        }
                                    );
                                }

                                if let Ok(sample_buffer) = alto_context.new_buffer(mono_samples, frame.sample_rate) {
                                    bgm_source.queue_buffer(sample_buffer).unwrap();
                                }
                            } else if frame.channels == 2 {     //Stereo
                                let mut stereo_samples = Vec::with_capacity(frame.data.len());
                                for i in (0..frame.data.len()).step_by(2) {
                                    stereo_samples.push(
                                        alto::Stereo {
                                            left: frame.data[i],
                                            right: frame.data[i + 1]
                                        }
                                    );
                                }

                                if let Ok(sample_buffer) = alto_context.new_buffer(stereo_samples, frame.sample_rate) {
                                    bgm_source.queue_buffer(sample_buffer).unwrap();
                                }
                            } else {
                                println!("Audio file must be mono or stereo.");
                                return;
                            }
                        }
                        Err(e) => {
                            match e {
                                mp3::Error::Eof => {
                                    println!("Looping the bgm");
                                    decoder.reader_mut().seek(SeekFrom::Start(0)).unwrap();
                                }
                                _ => { println!("Error decoding mp3 frame: {}", e); }
                            }
                        }
                    }
                }
            }

            //Match sfx pitches with the bgm pitch
            let pitch = bgm_source.pitch();
            for sfx in &mut active_sfx {
                let source = &mut sfx.source;
                if source.state() == SourceState::Playing {
                    if let Err(e) = source.set_pitch(pitch) {
                        println!("Error setting audio source pitch: {}", e);
                    }
                }
            }

            //Unqueue any processed buffers
            while bgm_source.buffers_processed() > 0 {
                bgm_source.unqueue_buffer().unwrap();
            }

            if bgm_source.state() != SourceState::Playing && start_bgm && bgm_source.buffers_queued() == IDEAL_FRAMES_QUEUED {
                bgm_source.play();
                start_bgm = false;
            }

            //Sleep for 10ms to avoid throttling the CPU core
            thread::sleep(Duration::from_millis(10));
        }
    });
}