#![allow(non_snake_case)]
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
    let window_size = glm::vec2(1200, 1200);
    let aspect_ratio = window_size.x as f32 / window_size.y as f32;
    let (mut window, events) = glfw.create_window(window_size.x, window_size.y, "OpenXR yay", glfw::WindowMode::Windowed).unwrap();
    window.set_resizable(false);
    window.set_key_polling(true);
    window.set_mouse_button_polling(true);
    window.set_cursor_pos_polling(true);

    //Load OpenGL function pointers
    gl::load_with(|symbol| window.get_proc_address(symbol));

    //OpenGL static configuration
	unsafe {
        gl::Enable(gl::CULL_FACE);										//Enable face culling
        gl::DepthFunc(gl::LESS);										//Pass the fragment with the smallest z-value.
        gl::Enable(gl::DEPTH_TEST);                                     //Enable depth testing
		gl::Enable(gl::FRAMEBUFFER_SRGB); 								//Enable automatic linear->SRGB space conversion
		gl::Enable(gl::BLEND);											//Enable alpha blending
		gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);			//Set blend func to (Cs * alpha + Cd * (1.0 - alpha))
        gl::ClearColor(0.53, 0.81, 0.92, 1.0);							//Set the clear color to a pleasant blue
        //gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);

		#[cfg(gloutput)]
		{
			gl::Enable(gl::DEBUG_OUTPUT);									//Enable verbose debug output
			gl::Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);						//Synchronously call the debug callback function
			gl::DebugMessageCallback(gl_debug_callback, ptr::null());		//Register the debug callback
			gl::DebugMessageControl(gl::DONT_CARE, gl::DONT_CARE, gl::DONT_CARE, 0, ptr::null(), gl::TRUE);
		}
    }
    
    //Compile shader programs
    let simple_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/simple.vert", "shaders/simple.frag") };
    let complex_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/mapped.vert", "shaders/mapped.frag") };

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

    let mut active_camera = false;
    let mut camera_position = glm::vec3(0.0, -5.0, 2.5);
    let mut camera_input: glm::TVec4<f32> = glm::zero();             //This is a unit vector in the xy plane in view space that represents the input camera movement vector
    let mut camera_orientation = glm::vec2(0.0, -glm::half_pi::<f32>());

    //Initialize view and projection matrices
    let mut view_matrix = glm::identity();
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

    let plane_mesh = {
        let plane_vertex_width = 2;
        let plane_index_count = (plane_vertex_width - 1) * (plane_vertex_width - 1) * 6;
        let plane_vao = ozy::prims::plane_vao(plane_vertex_width);  

        ozy::render::SimpleMesh::new(plane_vao, plane_index_count as GLint, "wood_veneer", &mut texture_keeper)
    };
    let plane_matrix = ozy::routines::uniform_scale(50.0);

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
                WindowEvent::Key(key, _, glfw::Action::Press, _) => {
                    match key {
                        glfw::Key::W => {
                            camera_input.z += -1.0;
                        }
                        glfw::Key::S => {
                            camera_input.z += 1.0;
                        }
                        glfw::Key::A => {
                            camera_input.x += -1.0;
                        }
                        glfw::Key::D => {
                            camera_input.x += 1.0;
                        }
                        _ => {}
                    }
                }
                WindowEvent::Key(key, _, glfw::Action::Release, _) => {
                    match key {
                        glfw::Key::W => {
                            camera_input.z -= -1.0;
                        }
                        glfw::Key::S => {
                            camera_input.z -= 1.0;
                        }
                        glfw::Key::A => {
                            camera_input.x -= -1.0;
                        }
                        glfw::Key::D => {
                            camera_input.x -= 1.0;
                        }
                        _ => {}
                    }
                }
                WindowEvent::MouseButton(glfw::MouseButtonRight, glfw::Action::Release, ..) => {
                    if active_camera {
                        window.set_cursor_mode(glfw::CursorMode::Normal);
                    } else {
                        window.set_cursor_mode(glfw::CursorMode::Hidden);
                    }
                    active_camera = !active_camera;
                }
                WindowEvent::CursorPos(x, y) => {
                    if active_camera {
                        const CAMERA_SENSITIVITY_DAMPENING: f32 = 0.002;
                        let offset = glm::vec2(x as f32 - window_size.x as f32 / 2.0, y as f32 - window_size.y as f32 / 2.0);
                        camera_orientation += offset * CAMERA_SENSITIVITY_DAMPENING;
                        if camera_orientation.y < -glm::pi::<f32>() {
                            camera_orientation.y = -glm::pi::<f32>();
                        } else if camera_orientation.y > 0.0 {
                            camera_orientation.y = 0.0;
                        }
                        println!("{:?}", camera_orientation);
                    }
                }
                _ => { println!("{:?}", event); }
            }
        }

        //Camera update
        if active_camera {
            window.set_cursor_pos(window_size.x as f64 / 2.0, window_size.y as f64 / 2.0);
        }
        const CAMERA_SPEED: f32 = 5.0;

        let camera_velocity = glm::vec4_to_vec3(&(glm::affine_inverse(view_matrix) * camera_input));
        camera_position += camera_velocity * delta_time * CAMERA_SPEED;

        //Construct matrices for rendering
        view_matrix = glm::rotation(camera_orientation.y, &glm::vec3(1.0, 0.0, 0.0)) *
                      glm::rotation(camera_orientation.x, &glm::vec3(0.0, 0.0, 1.0)) *
                      glm::translation(&(-camera_position));
        sphere_matrix = glm::translation(&glm::vec3(0.0, 0.0, f32::sin(1.2 * elapsed_time))) * glm::rotation(elapsed_time, &glm::vec3(0.0, 0.0, 1.0));

        //Render
        unsafe {
            shadow_rendertarget.bind();
            default_framebuffer.bind();

            //Bind common uniforms
            let programs = [complex_3D, simple_3D];
            for program in &programs {
                ozy::glutil::bind_matrix4(*program, "shadow_matrix", &(shadow_projection * shadow_view));
                ozy::glutil::bind_vector4(*program, "sun_direction", &uniform_light);
                ozy::glutil::bind_int(*program, "shadow_map", ozy::render::TEXTURE_MAP_COUNT as GLint);
            }
            gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
            gl::BindTexture(gl::TEXTURE_2D, shadow_rendertarget.texture);

            let texture_map_names = ["albedo_map", "normal_map", "roughness_map"];
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Init texture samplers
                ozy::glutil::bind_int(simple_3D, texture_map_names[i], i as GLint);
                ozy::glutil::bind_int(complex_3D, texture_map_names[i], i as GLint);
            }

            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, plane_mesh.texture_maps[i]);
            }            
            ozy::glutil::bind_matrix4(simple_3D, "mvp", &(projection_matrix * view_matrix * plane_matrix));
            ozy::glutil::bind_matrix4(simple_3D, "model_matrix", &plane_matrix);
            gl::UseProgram(simple_3D);
            gl::BindVertexArray(plane_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, plane_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());

            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, sphere_mesh.texture_maps[i]);
            }            
            ozy::glutil::bind_matrix4(complex_3D, "mvp", &(projection_matrix * view_matrix * sphere_matrix));
            ozy::glutil::bind_matrix4(complex_3D, "model_matrix", &sphere_matrix);
            gl::UseProgram(complex_3D);
            gl::BindVertexArray(sphere_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, sphere_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());
        }

        window.swap_buffers();
    }
}
