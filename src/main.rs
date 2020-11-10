extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

use std::process::exit;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winapi::um::{winuser::GetWindowDC, wingdi::wglGetCurrentContext};
use xr::Graphics;

fn main() {
    //Initialize glfw
    let mut glfw = match glfw::init(glfw::FAIL_ON_ERRORS) {
        Ok(g) => { g }
        Err(e) => { panic!("{}", e) }
    };
    
    //Ask for an OpenGL 4.3 core context
	glfw.window_hint(glfw::WindowHint::ContextVersion(4, 3));
	glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    //Create the window
    let (mut window, events) = glfw.create_window(800, 800, "OpenXR yay", glfw::WindowMode::Windowed).unwrap();

    gl::load_with(|symbol| window.get_proc_address(symbol));

    //Initialize the OpenXR instance
    let xr_instance = {
        let openxr_entry = xr::Entry::linked();
        let app_info = xr::ApplicationInfo {
            application_name: "hot_chickens",
            application_version: 1,
            engine_name: "ozy_engine",
            engine_version: 1
        };

        //Get the set of OpenXR extentions supported on this system
        let extension_set = match openxr_entry.enumerate_extensions() {
            Ok(set) => { set }
            Err(e) => { panic!("Extention enumerations error: {}", e); }
        };

        //Make sure the local OpenXR implementation supports OpenGL
        if !extension_set.khr_opengl_enable {
            println!("OpenXR implementation does not support OpenGL!");
            exit(-1);
        }
        
        //Create the instance. We're not using any additional API layers
        match openxr_entry.create_instance(&app_info, &extension_set, &[]) {
            Ok(inst) => { Some(inst) }
            Err(e) => { 
                println!("Error creating OpenXR instance: {}", e);
                None
            }
        }
    };

    //Initialize OpenXR session
    let xr_session = match xr_instance {
        Some(inst) => {
            let xr_systemid = match inst.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY) {
                Ok(id) => { Some(id) }
                Err(e) => { 
                    println!("Error getting OpenXR system: {}", e);
                    None
                }
            };

            match xr_systemid {
                Some(sysid) => unsafe {
                    let hwnd = match window.raw_window_handle() {
                        RawWindowHandle::Windows(handle) => {
                            handle.hwnd as winapi::shared::windef::HWND
                        }
                        _ => { panic!("Unsupported window system"); }
                    };
            
                    let session_create_info = xr::opengl::SessionCreateInfo::Windows {
                        h_dc: GetWindowDC(hwnd),
                        h_glrc: wglGetCurrentContext()
                    };

                    let sesh = match inst.create_session::<xr::OpenGL>(sysid, &session_create_info) {
                        Ok(s) => { Some(s) }
                        Err(e) => {
                            println!("Error initializing OpenXR session: {}", e);
                            None
                        }
                    };
                    sesh
                }
                None => {
                    None
                }
            }
        }
        None => { None }
    };

    
}
