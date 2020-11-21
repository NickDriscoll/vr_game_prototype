#![allow(non_snake_case)]
extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

mod structs;

use glfw::{Action, Context, Key, WindowEvent};
use gl::types::*;
use std::process::exit;
use std::ptr;
use std::time::Instant;
use rand::random;
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
    let window_size = glm::vec2(1280, 1024);
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
        gl::ClearColor(0.26, 0.4, 0.46, 1.0);							//Set the clear color to a pleasant blue
        //gl::ClearColor(0.0, 0.0, 0.0, 1.0);

		#[cfg(gloutput)]
		{
			gl::Enable(gl::DEBUG_OUTPUT);									//Enable verbose debug output
			gl::Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);						//Synchronously call the debug callback function
			gl::DebugMessageCallback(ozy::glutil::gl_debug_callback, ptr::null());		//Register the debug callback
			gl::DebugMessageControl(gl::DONT_CARE, gl::DONT_CARE, gl::DONT_CARE, 0, ptr::null(), gl::TRUE);
		}
    }
    
    //Compile shader programs
    let simple_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/simple.vert", "shaders/simple.frag") };
    let complex_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/mapped.vert", "shaders/mapped.frag") };
    let complex_instanced_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/mapped_instanced.vert", "shaders/mapped.frag") };
    let shadow_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/shadow.vert", "shaders/shadow.frag") };
    let shadow_instanced_3D = unsafe { ozy::glutil::compile_program_from_files("shaders/shadow_instanced.vert", "shaders/shadow.frag") };

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
    let mut camera_position = glm::vec3(0.0, -8.0, 5.5);
    let mut camera_input: glm::TVec4<f32> = glm::zero();             //This is a unit vector in the xy plane in view space that represents the input camera movement vector
    let mut camera_orientation = glm::vec2(0.0, -glm::half_pi::<f32>() * 0.6);
    let mut camera_speed = 5.0;

    //Initialize view and projection matrices
    let mut view_matrix = glm::identity();
    let projection_matrix = glm::perspective(aspect_ratio, glm::half_pi(), 0.1, 500.0);

    //Uniform light source
    let mut uniform_light = glm::normalize(&glm::vec4(-1.0, 0.0, 1.0, 0.0));

    //Initialize shadow data
    let mut shadow_view;
    let shadow_proj_size = 90.0;
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
    let plane_matrix = ozy::routines::uniform_scale(500.0);

    let sphere_block_width = 8;
    let sphere_block_sidelength = 40.0;
    let sphere_count = sphere_block_width * sphere_block_width;
    let sphere_mesh = ozy::render::SimpleMesh::from_ozy("models/sphere.ozy", &mut texture_keeper);
    let mut sphere_instanced_mesh = unsafe { ozy::render::InstancedMesh::from_simplemesh(&sphere_mesh, sphere_count, 5) };
    let mut sphere_transforms = vec![0.0; sphere_count * 16];

    //Create spheres
    let mut spheres = Vec::with_capacity(sphere_count);
    for i in 0..sphere_count {
        let rotation = if i == 0 {
            0.0
        } else {
            3.0 * random::<f32>()
        };
        let hover = 3.0 * random::<f32>();
        let sphere = structs::Sphere::new(rotation, hover);
        spheres.push(sphere)
    }

    //Main loop
    let mut last_frame_instant = Instant::now();
    let mut elapsed_time = 0.0;
    let mut wireframe = false;
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
                WindowEvent::Key(key, _, Action::Press, _) => {
                    match key {
                        Key::Q => unsafe {
                            if wireframe {
                                gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
                            } else {
                                gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
                            }
                            wireframe = !wireframe;
                        }
                        Key::W => {
                            camera_input.z += -1.0;
                        }
                        Key::S => {
                            camera_input.z += 1.0;
                        }
                        Key::A => {
                            camera_input.x += -1.0;
                        }
                        Key::D => {
                            camera_input.x += 1.0;
                        }
                        Key::LeftShift => {
                            camera_speed *= 5.0;
                        }
                        Key::LeftControl => {
                            camera_speed /= 5.0;
                        }
                        _ => {}
                    }
                }
                WindowEvent::Key(key, _, Action::Release, _) => {
                    match key {
                        Key::W => {
                            camera_input.z -= -1.0;
                        }
                        Key::S => {
                            camera_input.z -= 1.0;
                        }
                        Key::A => {
                            camera_input.x -= -1.0;
                        }
                        Key::D => {
                            camera_input.x -= 1.0;
                        }
                        Key::LeftShift => {
                            camera_speed /= 5.0;
                        }
                        Key::LeftControl => {
                            camera_speed *= 5.0;
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
                    }
                }
                _ => {  }
            }
        }

        //If the user is controlling the camera, force the mouse cursor into the center of the screen
        if active_camera {
            window.set_cursor_pos(window_size.x as f64 / 2.0, window_size.y as f64 / 2.0);
        }

        let camera_velocity = glm::vec4_to_vec3(&(glm::affine_inverse(view_matrix) * camera_input));
        camera_position += camera_velocity * delta_time * camera_speed;

        //Create a view matrix from the camera state
        view_matrix = glm::rotation(camera_orientation.y, &glm::vec3(1.0, 0.0, 0.0)) *
                      glm::rotation(camera_orientation.x, &glm::vec3(0.0, 0.0, 1.0)) *
                      glm::translation(&(-camera_position));

        //Update sphere transforms
        for i in 0..sphere_block_width {
            let ypos = sphere_block_sidelength * i as f32 / (sphere_block_width as f32 - 1.0) - sphere_block_sidelength / 2.0 + 20.0;

            for j in 0..sphere_block_width {
                let xpos = sphere_block_sidelength * j as f32 / (sphere_block_width as f32 - 1.0) - sphere_block_sidelength / 2.0;
                
                let sphere_index = i * sphere_block_width + j;

                let rotation = spheres[sphere_index].rotation_multiplier;
                let hover = spheres[sphere_index].hover_multiplier;

                let transform = glm::translation(&glm::vec3(xpos, ypos, 2.0 + f32::sin(hover * elapsed_time))) * 
                                glm::rotation(elapsed_time * rotation, &glm::vec3(0.0, 0.0, 1.0));

                //Write the transform to the buffer
                for k in 0..16 {
                    sphere_transforms[16 * sphere_index + k] = transform[k];
                }
            }
        }
        sphere_instanced_mesh.update_buffer(&sphere_transforms);

        //Make the light dance around
        uniform_light = glm::normalize(&glm::vec4(f32::cos(elapsed_time), f32::sin(elapsed_time), 2.0, 0.0));
        shadow_view = glm::look_at(&glm::vec4_to_vec3(&uniform_light), &glm::zero(), &glm::vec3(0.0, 0.0, 1.0));

        //Render
        unsafe {
            //Shadow map rendering
            shadow_rendertarget.bind();
            gl::UseProgram(shadow_instanced_3D);

            ozy::glutil::bind_matrix4(shadow_instanced_3D, "view_projection", &(shadow_projection * shadow_view));
            sphere_instanced_mesh.draw();

            //Main scene rendering
            default_framebuffer.bind();

            //Bind common uniforms
            let programs = [complex_3D, simple_3D, complex_instanced_3D];
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

            //Draw plane mesh
            ozy::glutil::bind_matrix4(simple_3D, "mvp", &(projection_matrix * view_matrix * plane_matrix));
            ozy::glutil::bind_matrix4(simple_3D, "model_matrix", &plane_matrix);
            ozy::glutil::bind_float(simple_3D, "uv_scale", 10.0);
            gl::UseProgram(simple_3D);
            gl::BindVertexArray(plane_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, plane_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());

            //Bind texture for the spheres
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, sphere_mesh.texture_maps[i]);
            }

            ozy::glutil::bind_matrix4(complex_instanced_3D, "view_projection", &(projection_matrix * view_matrix));
            ozy::glutil::bind_matrix4(complex_instanced_3D, "shadow_matrix", &(shadow_projection * shadow_view));
            gl::UseProgram(complex_instanced_3D);
            sphere_instanced_mesh.draw();
        }

        window.swap_buffers();
    }
}
