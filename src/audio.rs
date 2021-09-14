use alto::{sys::ALint, Source, SourceState};
use tfd::MessageBoxIcon;
use std::sync::Arc;
use std::collections::HashMap;
use std::fs::{File, read_dir};
use std::io::{Seek, SeekFrom};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use crate::structs::Configuration;

pub const DEFAULT_BGM_PATH: &str = "music/cryptic_relics.mp3";

const IDEAL_FRAMES_QUEUED: ALint = 5;

//Represents the kinds of messages the audio system can receive from the main thread
pub enum AudioCommand {
    SetListenerPosition([f32; 3]),
    SetListenerVelocity([f32; 3]),
    SetListenerOrientation(([f32; 3], [f32; 3])),
    SetListenerGain(f32),
    SetPitchShift(f32),
    LoadSFX(String),
    PlaySFX(String, [f32; 3]),
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
fn set_linearized_gain(ctxt: &alto::Context, linear_gain: f32) {
    let gain_factor = (f32::exp(linear_gain / 100.0) - 1.0) / (glm::e::<f32>() - 1.0);
    ctxt.set_gain(gain_factor).unwrap();
}

//Main function for the audio system
pub fn audio_main(audio_receiver: Receiver<AudioCommand>, bgm_volume: f32, config: &Configuration) {
    //Allocation is necessary here because we are moving this into another thread
    let mut bgm_path = match config.string_options.get(Configuration::MUSIC_NAME) {
        Some(path) => { String::from(path) }
        None => { String::from(DEFAULT_BGM_PATH) }
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
        let mut sfx_sources = Vec::with_capacity(STATIC_SOURCE_LIMIT);
        for _ in 0..STATIC_SOURCE_LIMIT {
            sfx_sources.push(alto_context.new_static_source().unwrap());
        }

        //Initialize the mp3 decoder with the default bgm
        let mut decoder = load_decoder(&bgm_path);
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
                                            mp3::Error::Eof => {
                                                println!("Done loading {}", path);
                                            }
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
                    AudioCommand::PlaySFX(path, position) => {
                        match sfx_buffers.get(&path) {
                            Some(buffer) => {
                                let mut available = false;
                                for source in &mut sfx_sources {
                                    if source.state() != SourceState::Playing {
                                        source.set_position(position).unwrap();
                                        source.set_buffer(buffer.clone()).unwrap();
                                        source.set_gain(20.0).unwrap();
                                        source.play();
                                        available = true;
                                        break;
                                    }
                                }

                                if !available {
                                    println!("No available sfx slot to play {}", path);
                                }
                            }
                            None => {
                                println!("{} hasn't been loaded yet", path);
                            }
                        }
                    }
                    AudioCommand::SelectNewBGM => {
                        bgm_source.pause();
                        match tfd::open_file_dialog("Choose bgm", "music/", Some((&["*.mp3"], "mp3 files (*.mp3)"))) {
                            Some(path) => {
                                let pitch = bgm_source.pitch();
                                bgm_source.stop();
                                decoder = load_decoder(&path);
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
                        if let Some(_) = &mut decoder {
                            bgm_source.stop();
                            decoder = load_decoder(&bgm_path);
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
                if let Some(decoder) = &mut decoder {
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
            for source in &mut sfx_sources {
                source.set_pitch(pitch).unwrap();
            }

            //Unqueue any processed buffers
            while bgm_source.buffers_processed() > 0 {
                bgm_source.unqueue_buffer().unwrap();
            }

            if bgm_source.state() != SourceState::Playing && start_bgm && bgm_source.buffers_queued() == IDEAL_FRAMES_QUEUED {
                bgm_source.play();
                start_bgm = false;
            }

            //Sleeping to avoid throttling the CPU core
            thread::sleep(Duration::from_millis(10));
        }
    });
}