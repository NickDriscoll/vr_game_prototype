extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

use glfw::{Context, WindowEvent};
use std::process::exit;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winapi::um::{winuser::GetWindowDC, wingdi::wglGetCurrentContext};

fn main() {
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

    //Get the system id for the HMD
    let xr_systemid = match &xr_instance {
        Some(inst) => {
            match inst.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY) {
                Ok(id) => { Some(id) }
                Err(e) => { 
                    println!("Error getting OpenXR system: {}", e);
                    None
                }
            }
        }
        None => { None }
    };

    let reqs = match &xr_instance {
        Some(inst) => {
            match xr_systemid {
                Some(sysid) => {
                    match inst.graphics_requirements::<xr::OpenGL>(sysid) {
                        Ok(reqs) => { Some(reqs) }
                        Err(e) => {
                            println!("Couldn't get graphics requirements: {}", e);
                            None
                        }
                    }
                }
                None => { None }
            }
        }
        None => { None }
    };

    //Initialize glfw
    let mut glfw = match glfw::init(glfw::FAIL_ON_ERRORS) {
        Ok(g) => { g }
        Err(e) => { panic!("{}", e) }
    };
    
    //Ask for an OpenGL version based on what OpenXR says. Default to 4.3
    match reqs {
        Some(r) => {
            glfw.window_hint(glfw::WindowHint::ContextVersion(r.min_api_version_supported.major() as u32, r.min_api_version_supported.minor() as u32));
        }
        None => {
            glfw.window_hint(glfw::WindowHint::ContextVersion(4, 3));
        }
    }    
	glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    //Create the window
    let (mut window, events) = glfw.create_window(800, 800, "OpenXR yay", glfw::WindowMode::Windowed).unwrap();

    //Load OpenGL function pointers
    gl::load_with(|symbol| window.get_proc_address(symbol));

    //OpenGL static configuration
	unsafe {
		gl::Enable(gl::CULL_FACE);										//Enable face culling
		gl::DepthFunc(gl::LESS);										//Pass the fragment with the smallest z-value.
		gl::Enable(gl::FRAMEBUFFER_SRGB); 								//Enable automatic linear->SRGB space conversion
		gl::Enable(gl::BLEND);											//Enable alpha blending
		gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);			//Set blend func to (Cs * alpha + Cd * (1.0 - alpha))
		gl::ClearColor(0.53, 0.81, 0.92, 1.0);							//Set the clear color to a pleasant blue

		#[cfg(gloutput)]
		{
			gl::Enable(gl::DEBUG_OUTPUT);									//Enable verbose debug output
			gl::Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);						//Synchronously call the debug callback function
			gl::DebugMessageCallback(gl_debug_callback, ptr::null());		//Register the debug callback
			gl::DebugMessageControl(gl::DONT_CARE, gl::DONT_CARE, gl::DONT_CARE, 0, ptr::null(), gl::TRUE);
		}
	}

    //Initialize OpenXR session
    let xr_session = match xr_instance {
        Some(inst) => {
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
                None => { None }
            }
        }
        None => { None }
    };

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Close => { window.set_should_close(true); }
                _ => {}
            }
        }

        //window.swap_buffers();
    }
}
