//#![w]
extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

use glfw::{Context, WindowEvent};
use gl::types::*;
use std::process::exit;
use std::ptr;
use std::time::Instant;
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
    drop(reqs);
	glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    //Create the window
    let window_size = glm::vec2(800, 800);
    let aspect_ratio = window_size.x as f32 / window_size.y as f32;
    let (mut window, events) = glfw.create_window(window_size.x, window_size.y, "OpenXR yay", glfw::WindowMode::Windowed).unwrap();
    window.set_resizable(false);

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
    
    //Compile shader programs
    let program_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/mapped.vert", "shaders/mapped.frag") };

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
    
    //Initialize default framebuffer
    let default_framebuffer = ozy::render::Framebuffer {
        name: 0,
        size: (window_size.x as GLsizei, window_size.y as GLsizei),
        clear_flags: gl::DEPTH_BUFFER_BIT | gl::COLOR_BUFFER_BIT,
        cull_face: gl::BACK
    };

    //Initialize view and projection matrices
    let view_marix = glm::look_at(&glm::vec3(0.0, 1.5, 1.5), &glm::vec3(0.0, 0.0, 0.0), &glm::vec3(0.0, 0.0, 1.0));
    let projection_matrix = glm::perspective(aspect_ratio, glm::half_pi(), 0.1, 500.0);

    //Uniform light source
    let uniform_light = glm::normalize(&glm::vec4(-1.0, 0.0, 1.0, 0.0));

    //Initialize shadow data
    let shadow_view = glm::look_at(&glm::vec4_to_vec3(&uniform_light), &glm::zero(), &glm::vec3(0.0, 0.0, 1.0));
    let shadow_proj_size = 3.0;
    let shadow_projection = glm::ortho(-shadow_proj_size, shadow_proj_size, -shadow_proj_size, shadow_proj_size, -shadow_proj_size, 2.0 * shadow_proj_size);
    let shadow_size = 8192;
    let shadow_rendertarget = unsafe { ozy::render::RenderTarget::new_shadow((shadow_size, shadow_size)) };

    //Initialize texture caching struct
    let mut texture_keeper = ozy::render::TextureKeeper::new();

    let mut sphere_matrix = glm::identity();
    let sphere_mesh = ozy::render::SimpleMesh::from_ozy("models/sphere.ozy", &mut texture_keeper);

    //Main loop
    let mut last_frame_instant = Instant::now();
    let mut elapsed_time = 0.0;
    while !window.should_close() {
        let delta_time = {
			let frame_instant = Instant::now();
			let dur = frame_instant.duration_since(last_frame_instant);
			last_frame_instant = frame_instant;
			dur.as_secs_f32()
        };
        elapsed_time += delta_time;

        //Poll window events and handle them
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Close => { window.set_should_close(true); }
                _ => {}
            }
        }


        sphere_matrix = glm::rotation(elapsed_time, &glm::vec3(0.0, 0.0, 1.0));

        //Render
        unsafe {
            shadow_rendertarget.bind();
            default_framebuffer.bind();

            ozy::glutil::bind_matrix4(program_3D, "mvp", &(projection_matrix * view_marix * sphere_matrix));
            ozy::glutil::bind_matrix4(program_3D, "model_matrix", &sphere_matrix);
            ozy::glutil::bind_matrix4(program_3D, "shadow_matrix", &(shadow_projection * shadow_view));
            ozy::glutil::bind_vector4(program_3D, "sun_direction", &uniform_light);

            let texture_map_names = ["albedo_map", "normal_map", "roughness_map"];
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Init texture samplers
                ozy::glutil::bind_int(program_3D, texture_map_names[i], i as GLint);

                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, sphere_mesh.texture_maps[i]);
            }
            ozy::glutil::bind_int(program_3D, "shadow_map", ozy::render::TEXTURE_MAP_COUNT as GLint);
            gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
            gl::BindTexture(gl::TEXTURE_2D, shadow_rendertarget.texture);
            
            gl::BindVertexArray(sphere_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, sphere_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());
        }

        window.swap_buffers();
    }
}
