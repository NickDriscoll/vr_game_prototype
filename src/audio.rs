use alto::{sys::ALint, Source, SourceState};
use tfd::MessageBoxIcon;
use std::fs::File;
use std::io::{ErrorKind, Seek, SeekFrom};
use std::sync::mpsc::Receiver;
use std::process::exit;
use std::thread;
use std::time::Duration;
use crate::structs::Configuration;

const DEFAULT_BGM_PATH: &str = "music/ikebukuro.mp3";
const IDEAL_FRAMES_QUEUED: ALint = 10;

//Represents the kinds of messages the audio system can receive from the 
pub enum AudioCommand {
    SetListenerPosition([f32; 3]),
    SetListenerVelocity([f32; 3]),
    SetListenerOrientation(([f32; 3], [f32; 3])),
    SetSourcePosition([f32; 3], usize),
    SetListenerGain(f32),
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

fn set_linearized_gain(ctxt: &alto::Context, volume: f32) {
    let gain_factor = (f32::exp(volume / 100.0) - 1.0) / (glm::e::<f32>() - 1.0);
    ctxt.set_gain(gain_factor).unwrap();
}

//Main function for the audio system
pub fn audio_main(audio_receiver: Receiver<AudioCommand>, bgm_volume: f32, conf: &Configuration) {
    //Allocation is necessary here because we are moving this into another thread
    let default_bgm = match conf.string_options.get(Configuration::MUSIC_NAME) {
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

        //Initialize the mp3 decoder with the default bgm
        let mut decoder = load_decoder(&default_bgm);

        let mut kanye_source = alto_context.new_streaming_source().unwrap();
        let mut kickstart_bgm = true;
        loop {
            //Process all commands from the main thread
            while let Ok(command) = audio_receiver.try_recv() {
                match command {
                    AudioCommand::SetListenerPosition(pos) => { alto_context.set_position(pos).unwrap(); }
                    AudioCommand::SetListenerVelocity(vel) => { alto_context.set_velocity(vel).unwrap(); }
                    AudioCommand::SetListenerOrientation(ori) => { alto_context.set_orientation(ori).unwrap(); }
                    AudioCommand::SetSourcePosition(pos, i) => { if i == 0 { kanye_source.set_position(pos).unwrap(); } }
                    AudioCommand::SetListenerGain(volume) => { set_linearized_gain(&alto_context, volume); }
                    AudioCommand::SelectNewBGM => {
                        kanye_source.pause();
                        match tfd::open_file_dialog("Choose bgm", "music/", Some((&["*.mp3"], "mp3 files (*.mp3)"))) {
                            Some(res) => {
                                kanye_source.stop();
                                decoder = load_decoder(&res);
                            
                                //Clear out any residual sound data from the old mp3
                                kanye_source = alto_context.new_streaming_source().unwrap();
                                kickstart_bgm = true;
                            }
                            None => { kanye_source.play(); }
                        }
                    }
                    AudioCommand::RestartBGM => {
                        println!("Looping the mp3");
                        
                        //Dequeue any processed buffers
                        while kanye_source.buffers_processed() > 0 {
                            kanye_source.unqueue_buffer().unwrap();
                        }

                        kanye_source.pause();
                        if let Some(decoder) = &mut decoder {
                            kanye_source = alto_context.new_streaming_source().unwrap();
                            decoder.reader_mut().seek(SeekFrom::Start(0)).unwrap();
                        }
                        kickstart_bgm = true;
                    }
                    AudioCommand::PlayPause => {
                        kickstart_bgm = !kickstart_bgm;
                        match kanye_source.state() {
                            SourceState::Playing | SourceState::Initial => {
                                kanye_source.pause();                                
                                kickstart_bgm = false;
                            }
                            SourceState::Paused | SourceState::Stopped => {
                                kanye_source.play();
                                kickstart_bgm = true;
                            }
                            SourceState::Unknown(code) => { println!("Source is in an unknown state: {}", code); }
                        }
                    }
                }
            }

            //If there are fewer than the ideal number of frames queued, prepare and queue a frame
            if kanye_source.buffers_queued() < IDEAL_FRAMES_QUEUED {
                if let Some(decoder) = &mut decoder {
                    match decoder.next_frame() {
                        Ok(frame) => {                          //Mono
                            if frame.channels == 1 {
                                let mut mono_samples = Vec::with_capacity(frame.data.len());
                                for sample in frame.data {
                                    mono_samples.push(
                                        alto::Mono {
                                            center: sample
                                        }
                                    );
                                }

                                if let Ok(sample_buffer) = alto_context.new_buffer(mono_samples, frame.sample_rate) {
                                    kanye_source.queue_buffer(sample_buffer).unwrap();
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
                                    kanye_source.queue_buffer(sample_buffer).unwrap();
                                }
                            } else {
                                println!("Audio file must have one or two channels.");
                                return;
                            }
                        }
                        Err(e) => {
                            match e {
                                mp3::Error::Eof => {
                                    println!("Looping the mp3");
                                    decoder.reader_mut().seek(SeekFrom::Start(0)).unwrap();
                                }
                                _ => { println!("Error decoding mp3 frame: {}", e); }
                            }
                        }
                    }
                }
            }

            //Unqueue any processed buffers
            while kanye_source.buffers_processed() > 0 {
                kanye_source.unqueue_buffer().unwrap();
            }

            if kanye_source.state() != SourceState::Playing && kickstart_bgm && kanye_source.buffers_queued() > 0 {
                kanye_source.play();
                kickstart_bgm = false;
            }

            //Sleeping to avoid throttling a CPU core
            thread::sleep(Duration::from_millis(10));
        }
    });
}