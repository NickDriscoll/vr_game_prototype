#![allow(non_snake_case)]
extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

mod structs;

use structs::{Command, Sphere};

use glfw::{Action, Context, Key, WindowEvent};
use gl::types::*;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::process::exit;
use std::ptr;
use std::time::Instant;
use rand::random;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use ozy::{glutil};
use xr::sys::{Bool32, DebugUtilsMessengerEXT, DebugUtilsMessengerCallbackDataEXT, DebugUtilsMessengerCreateInfoEXT, DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT};

#[cfg(windows)]
use winapi::um::{winuser::GetWindowDC, wingdi::wglGetCurrentContext};

use ozy::render::{ScreenState, SimpleMesh};

const FONT_BYTES: &'static [u8; 212276] = include_bytes!("../fonts/Constantia.ttf");

unsafe extern "system" fn xr_debug_callback(severity_flags: DebugUtilsMessageSeverityFlagsEXT, type_flags: DebugUtilsMessageTypeFlagsEXT, callback_data: *const DebugUtilsMessengerCallbackDataEXT, user_data: *mut c_void) -> Bool32 {
    println!("---------------------------OpenXR Debug Message---------------------------");

    if severity_flags.contains(DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        println!("Severity: ERROR");
    } else if severity_flags.contains(DebugUtilsMessageSeverityFlagsEXT::WARNING) {        
        println!("Severity: WARNING");
    } else if severity_flags.contains(DebugUtilsMessageSeverityFlagsEXT::INFO) {        
        println!("Severity: INFO");
    } else if severity_flags.contains(DebugUtilsMessageSeverityFlagsEXT::VERBOSE) {        
        println!("Severity: VERBOSE");
    }

    if type_flags.contains(DebugUtilsMessageTypeFlagsEXT::GENERAL) {
        println!("Type: GENERAL");
    } else if type_flags.contains(DebugUtilsMessageTypeFlagsEXT::VALIDATION) {
        println!("Type: VALIDATION");
    } else if type_flags.contains(DebugUtilsMessageTypeFlagsEXT::PERFORMANCE) {
        println!("Type: PERFORMANCE");
    } else if type_flags.contains(DebugUtilsMessageTypeFlagsEXT::CONFORMANCE) {
        println!("Type: CONFORMANCE");
    }

    let message_id = CStr::from_ptr((*callback_data).message_id);
    
    let f_name = CStr::from_ptr((*callback_data).function_name);

    let message = CStr::from_ptr((*callback_data).message);

    println!("Function name: {:?}\nMessage ID: {:?}\nMessage: {:?}", f_name, message_id, message);
    drop(message_id);
    drop(f_name);
    drop(message);


    println!("--------------------------------------------------------------------------");
    Bool32::from(true)
}

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

        if let Ok(layer_properties) = openxr_entry.enumerate_layers() {
            println!("API layers:");
            for layer in layer_properties.iter() {
                println!("{}", layer.layer_name);
            }
        }
        
        //Create the instance
        #[cfg(xrdebug)]
        let mut messenger = DebugUtilsMessengerEXT::NULL;
        match openxr_entry.create_instance(&app_info, &extension_set, &[]) {
            Ok(inst) => unsafe {
                //Enable the OpenXR debug extension
                #[cfg(xrdebug)]
                {
                    match xr::raw::DebugUtilsEXT::load(&openxr_entry, inst.as_raw()) {
                        Ok(debug_utils) => {
                            let debug_createinfo = DebugUtilsMessengerCreateInfoEXT {
                                ty: xr::sys::StructureType::DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT,
                                next: ptr::null(),
                                message_severities: DebugUtilsMessageSeverityFlagsEXT::VERBOSE | DebugUtilsMessageSeverityFlagsEXT::WARNING | DebugUtilsMessageSeverityFlagsEXT::INFO | DebugUtilsMessageSeverityFlagsEXT::ERROR,
                                message_types: DebugUtilsMessageTypeFlagsEXT::GENERAL | DebugUtilsMessageTypeFlagsEXT::VALIDATION | DebugUtilsMessageTypeFlagsEXT::PERFORMANCE | DebugUtilsMessageTypeFlagsEXT::CONFORMANCE,
                                user_callback: Some(xr_debug_callback),
                                user_data: ptr::null_mut()
                            };
                            
                            (debug_utils.create_debug_utils_messenger)(inst.as_raw(), &debug_createinfo as *const _, &mut messenger as *mut _);
                        }
                        Err(e) => {
                            println!("Couldn't load OpenXR debug utils: {}", e);
                        }
                    }
                }
                Some(inst)
            }
            Err(e) => { 
                println!("Error creating OpenXR instance: {}", e);
                None
            }
        }
    };

    //Get the system id
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

    let xr_viewconfiguration_views = match (&xr_instance, xr_systemid) {
        (Some(inst), Some(sys_id)) => {
            match inst.enumerate_view_configuration_views(sys_id, xr::ViewConfigurationType::PRIMARY_STEREO) {
                Ok(vcvs) => { Some(vcvs) }
                Err(e) => {
                    println!("Couldn't get ViewConfigurationViews: {}", e);
                    None
                }
            }
        }
        _ => { None }
    };

    //Get the max swapchain size
    let xr_swapchain_size = match &xr_viewconfiguration_views {
        Some(views) => { Some(glm::vec2(views[0].recommended_image_rect_width, views[0].recommended_image_rect_height)) }
        _ => { None }
    };

    //Get the OpenXR runtime's OpenGL version requirements
    let xr_graphics_reqs = match &xr_instance {
        Some(inst) => {
            match xr_systemid {
                Some(sysid) => {
                    match inst.graphics_requirements::<xr::OpenGL>(sysid) {
                        Ok(reqs) => { Some(reqs) }
                        Err(e) => {
                            println!("Couldn't get OpenXR graphics requirements: {}", e);
                            None
                        }
                    }
                }
                None => { None }
            }
        }
        None => { None }
    };

    //Create the paths to appropriate equipment
    let left_hand_path = match &xr_instance {
        Some(instance) => {
            match instance.string_to_path(xr::USER_HAND_LEFT) {
                Ok(path) => { Some(path) }
                Err(e) => {
                    println!("Error getting XrPath: {}", e);
                    None
                }
            }
        }
        None => { None }
    };

    //Create the actionset
    let xr_controller_actionset = match &xr_instance {
        Some(inst) => {
            match inst.create_action_set("controllers", "Controllers", 0) {
                Ok(set) => { Some(set) }
                Err(e) => {
                    println!("Error creating XrActionSet: {}", e);
                    None
                }
            }
        }
        None => { None }
    };

    //Create the actions for getting pose data
    let controller_pose_action = match &xr_controller_actionset {
        Some(actionset) => {
            match left_hand_path {
                Some(path) => {
                    match actionset.create_action::<xr::Posef>("get_pose", "Get pose", &[path]) {
                        Ok(action) => { Some(action) }
                        Err(e) => {
                            println!("Error creating XrAction: {}", e);
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
    
    //Ask for an OpenGL version based on what OpenXR requests. Default to 4.3
    match xr_graphics_reqs {
        Some(r) => {
            glfw.window_hint(glfw::WindowHint::ContextVersion(r.min_api_version_supported.major() as u32, r.min_api_version_supported.minor() as u32));
        }
        None => {
            glfw.window_hint(glfw::WindowHint::ContextVersion(4, 3));
        }
    }
	glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    //Create the window
    let window_size = match &xr_swapchain_size {
        Some(size) => { 
            glm::vec2(
                size.x / 2,
                size.y / 2
            )
        }
        None => { glm::vec2(1280, 1024) }
    };

    let aspect_ratio = window_size.x as f32 / window_size.y as f32;
    let (mut window, events) = match glfw.create_window(window_size.x, window_size.y, "OpenXR yay", glfw::WindowMode::Windowed) {
        Some(stuff) => { stuff }
        None => {
            panic!("Unable to create a window!");
        }
    };
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
		gl::Enable(gl::FRAMEBUFFER_SRGB); 								//Enable automatic linear->SRGB space conversion
        gl::Enable(gl::BLEND);											//Enable alpha blending
        gl::Enable(gl::MULTISAMPLE);                                    //Enable MSAA
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

    //Initialize OpenXR session
    let (xr_session, mut xr_framewaiter, mut xr_framestream): (Option<xr::Session<xr::OpenGL>>, Option<xr::FrameWaiter>, Option<xr::FrameStream<xr::OpenGL>>) = match &xr_instance {
        Some(inst) => {
            match xr_systemid {
                Some(sysid) => unsafe {
                    #[cfg(windows)] {
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

                        match inst.create_session::<xr::OpenGL>(sysid, &session_create_info) {
                            Ok(sesh) => {
                                match sesh.0.begin(xr::ViewConfigurationType::PRIMARY_STEREO) {
                                    Ok(_) => { (Some(sesh.0), Some(sesh.1), Some(sesh.2)) }
                                    Err(e) => {
                                        println!("Error beginning XrSession: {}", e);
                                        (None, None, None)
                                    }
                                }                            
                            }
                            Err(e) => {
                                println!("Error initializing OpenXR session: {}", e);
                                (None, None, None)
                            }
                        }
                    }

                    #[cfg(unix)] {
                        (None, None, None)
                    }
                }
                None => { (None, None, None) }
            }
        }
        None => { (None, None, None) }
    };

    //Set controller actionset as active
    match (&xr_session, &xr_controller_actionset) {
        (Some(session), Some(actionset)) => {
            if let Err(e) = session.attach_action_sets(&[&actionset]) {
                println!("Unable to sync actions: {}", e);
            }
        }
        _ => {}
    }

    //Create tracking space
    let tracking_space = match &xr_session {
        Some(session) => {
            match session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY) {
                Ok(space) => { Some(space) }
                Err(e) => {
                    println!("Couldn't create reference space: {}", e);
                    None
                }
            }
        }
        None => { None }
    };

    //Create swapchains
    let mut xr_swapchains = match (&xr_session, &xr_swapchain_size, &xr_viewconfiguration_views) {
        (Some(session), Some(size), Some(viewconfig_views)) => {
            let mut failed = false;
            let mut scs = Vec::with_capacity(viewconfig_views.len());
            for viewconfig in viewconfig_views {
                let create_info = xr::SwapchainCreateInfo {
                    create_flags: xr::SwapchainCreateFlags::EMPTY,
                    usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                    format: gl::SRGB8_ALPHA8,
                    sample_count: viewconfig.recommended_swapchain_sample_count,
                    width: viewconfig.recommended_image_rect_width,
                    height: viewconfig.recommended_image_rect_height,
                    face_count: 1,
                    array_size: 1,
                    mip_count: 1
                };
    
                match session.create_swapchain(&create_info) {
                    Ok(sc) => { scs.push(sc); }
                    Err(e) => {
                        println!("Error creating swapchain: {}", e); 
                        failed = true;
                        break;
                    }
                }
            }

            if failed { None }
            else { Some(scs) }
        }
        _ => { None }
    };

    //Create swapchain framebuffer
    let xr_swapchain_framebuffer = unsafe {
        let mut p = 0;
        gl::GenFramebuffers(1, &mut p);
        p
    };

    let mut xr_image_count = 0;
    let xr_swapchain_images = match &xr_swapchains {
        Some(chains) => {
            let mut failed = false;
            let mut image_arr = Vec::with_capacity(chains.len());
            for chain in chains.iter() {
                match chain.enumerate_images() {
                    Ok(images) => {
                        xr_image_count += images.len();
                        image_arr.push(images);
                    }
                    Err(e) => {
                        println!("Error getting swapchain images: {}", e);
                        failed = true;
                        break;
                    }
                }
            }

            if failed { None }
            else { Some(image_arr) }
        }
        None => { None }
    };    
    let mut xr_depth_textures = vec![None; xr_image_count];

    //Compile shader programs
    let complex_3D = unsafe { glutil::compile_program_from_files("shaders/mapped.vert", "shaders/mapped.frag") };
    let complex_instanced_3D = unsafe { glutil::compile_program_from_files("shaders/mapped_instanced.vert", "shaders/mapped.frag") };
    let shadow_3D = unsafe { glutil::compile_program_from_files("shaders/shadow.vert", "shaders/shadow.frag") };
    let shadow_instanced_3D = unsafe { glutil::compile_program_from_files("shaders/shadow_instanced.vert", "shaders/shadow.frag") };
    
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
    let camera_hit_sphere_radius = 0.2;

    //Initialize screen state
    let mut screen_state = ScreenState::new(window_size, glm::identity(), glm::perspective_zo(aspect_ratio, glm::half_pi(), 0.1, 500.0));

    //Uniform light source
    let mut uniform_light = glm::normalize(&glm::vec4(1.0, 0.0, 1.0, 0.0));

    //Initialize shadow data
    let mut shadow_view;
    let shadow_proj_size = 90.0;
    let shadow_projection = glm::ortho(-shadow_proj_size, shadow_proj_size, -shadow_proj_size, shadow_proj_size, -shadow_proj_size, 2.0 * shadow_proj_size);
    let shadow_size = 8192;
    let shadow_rendertarget = unsafe { ozy::render::RenderTarget::new_shadow((shadow_size, shadow_size)) };

    //Initialize texture caching struct
    let mut texture_keeper = ozy::render::TextureKeeper::new();
    let tex_params = [
        (gl::TEXTURE_WRAP_S, gl::REPEAT),
	    (gl::TEXTURE_WRAP_T, gl::REPEAT),
	    (gl::TEXTURE_MIN_FILTER, gl::LINEAR),
	    (gl::TEXTURE_MAG_FILTER, gl::LINEAR)
    ];

    let mut mouse_lbutton_pressed = false;
    let mut mouse_lbutton_pressed_last_frame = false;
    let mut screen_space_mouse = glm::zero();

    //Initialize UI system
    let pause_menu_index = 0;
    let graphics_menu_index = 1;
    let pause_menu_chain_index;
    let graphics_menu_chain_index;
    let mut ui_state = {
        let button_program = unsafe { ozy::glutil::compile_program_from_files("shaders/button.vert", "shaders/button.frag") };
        let glyph_program = unsafe { ozy::glutil::compile_program_from_files("shaders/glyph.vert", "shaders/glyph.frag") };

        let mut state = ozy::ui::UIState::new(FONT_BYTES, (window_size.x, window_size.y), [button_program, glyph_program]);
        pause_menu_chain_index = state.create_menu_chain();
        graphics_menu_chain_index = state.create_menu_chain();
        
        let menus = vec![
            ozy::ui::Menu::new(vec![
                ("Quit", Some(Command::Quit)),
                ("Graphics options", Some(Command::ToggleMenu(graphics_menu_chain_index, 1)))
            ], ozy::ui::UIAnchor::LeftAligned((20.0, 20.0)), 24.0),
            ozy::ui::Menu::new(vec![
                ("Highlight spheres", Some(Command::ToggleOutline)),
                ("Visualize normals", Some(Command::ToggleNormalVis)),
                ("Complex normals", Some(Command::ToggleComplexNormals)),
                ("Wireframe view", Some(Command::ToggleWireframe))
            ], ozy::ui::UIAnchor::LeftAligned((20.0, window_size.y as f32 / 2.0)), 24.0)
        ];

        state.set_menus(menus);
        state.toggle_menu(pause_menu_chain_index, pause_menu_index);
        state
    };

    let plane_mesh = {
        let plane_vertex_width = 2;
        let plane_index_count = (plane_vertex_width - 1) * (plane_vertex_width - 1) * 6;
        let plane_vao = ozy::prims::plane_vao(plane_vertex_width);  

        ozy::render::SimpleMesh::new(plane_vao, plane_index_count as GLint, "tiles", &mut texture_keeper, &tex_params)
    };
    let plane_matrix = ozy::routines::uniform_scale(200.0);

    let sphere_block_scale = 1;
    let sphere_block_width = 8 * sphere_block_scale;
    let sphere_block_sidelength = 40.0 * sphere_block_scale as f32;
    let sphere_count = sphere_block_width * sphere_block_width;
    let sphere_mesh = SimpleMesh::from_ozy("models/sphere.ozy", &mut texture_keeper, &tex_params);
    let mut sphere_instanced_mesh = unsafe { ozy::render::InstancedMesh::from_simplemesh(&sphere_mesh, sphere_count, 5) };
    let mut sphere_transforms = vec![0.0; sphere_count * 16];

    //Create spheres    
    let mut spheres = Vec::with_capacity(sphere_count);
    for i in 0..sphere_count {
        let randomness_multiplier = 4.0;
        let (rotation, hover) = if i == 0 {
            (0.0, 0.0)
        } else {
            (2.0 * randomness_multiplier * random::<f32>(), randomness_multiplier * random::<f32>())
        };
        let sphere = Sphere::new(rotation, hover);
        spheres.push(sphere)
    }

    //Create teapot
    let teapot_mesh = SimpleMesh::from_ozy("models/teapot.ozy", &mut texture_keeper, &tex_params);
    let mut teapot_matrix;

    let mut visualize_normals = false;
    let mut complex_normals = false;
    let mut wireframe = false;
    let mut outlining = false;

    //Main loop
    let mut last_frame_instant = Instant::now();
    let mut elapsed_time = 0.0;
    let mut command_buffer = Vec::new();
    while !window.should_close() {
        let delta_time = {
			let frame_instant = Instant::now();
			let dur = frame_instant.duration_since(last_frame_instant);
			last_frame_instant = frame_instant;
			dur.as_secs_f32()
        };
        elapsed_time += delta_time;
        mouse_lbutton_pressed_last_frame = mouse_lbutton_pressed;

        //Get data from the VR hardware
        /*
        let left_hand_pose = match (&xr_session, &tracking_space, left_hand_path, &controller_pose_action) {
            (Some(session), Some(t_space), Some(path), Some(action)) => {
                match action.create_space(session.clone(), path, xr::Posef::IDENTITY) {
                    Ok(space) => {
                        match space.locate(t_space, ) {
                            Ok(space_location) => {
                                Some(space_location.pose)
                            }
                            Err(e) => {
                                println!("Couldn't locate space: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        println!("Couldn't get left hand space: {}", e);
                        None
                    }
                }
            }
            _ => { None }
        };
        
        match left_hand_pose {
            Some(pose) => {
                println!("Position: ({}, {}, {})", pose.position.x, pose.position.y, pose.position.z);
            }
            None => {
                println!("Pose was none");
            }
        }

        //Poll for OpenXR events
        if let Some(instance) = &xr_instance {
            let mut buffer = xr::EventDataBuffer::new();
            if let Ok(Some(event)) = instance.poll_event(&mut buffer) {
                
            }
        }
        */

        //Poll window events and handle them
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Close => { window.set_should_close(true); }
                WindowEvent::Key(key, _, Action::Press, _) => {
                    match key {
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
                WindowEvent::MouseButton(glfw::MouseButtonLeft, action, ..) => {
                    if action == glfw::Action::Press {
                        mouse_lbutton_pressed = true;
                    } else {
                        mouse_lbutton_pressed = false;
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
                    screen_space_mouse = glm::vec2(x as f32, y as f32);
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

        //Update the state of the ui
        ui_state.update_buttons(screen_space_mouse, mouse_lbutton_pressed, mouse_lbutton_pressed_last_frame, &mut command_buffer);

        //Drain the command_buffer and process commands
        for command in command_buffer.drain(0..command_buffer.len()) {
            match command {
                Command::Quit => { window.set_should_close(true); }
                Command::ToggleMenu(chain_index, menu_index) => { ui_state.toggle_menu(chain_index, menu_index); }
                Command::ToggleNormalVis => { visualize_normals = !visualize_normals; }
                Command::ToggleComplexNormals => { complex_normals = !complex_normals; }                
                Command::ToggleOutline => { outlining = !outlining; }
                Command::ToggleWireframe => unsafe {
                    if wireframe {
                        gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
                    } else {
                        gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
                    }
                    wireframe = !wireframe;
                }
            }
        }

        //If the user is controlling the camera, force the mouse cursor into the center of the screen
        if active_camera {
            window.set_cursor_pos(window_size.x as f64 / 2.0, window_size.y as f64 / 2.0);
        }

        let camera_velocity = glm::vec4_to_vec3(&(glm::affine_inverse(*screen_state.get_view_from_world()) * camera_input));
        camera_position += camera_velocity * delta_time * camera_speed;

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

        teapot_matrix = glm::translation(&glm::vec3(0.0, 4.0, 0.0)) * glm::rotation(elapsed_time, &glm::vec3(0.0, 0.0, 1.0)) * glm::translation(&glm::vec3(3.0, 0.0, 6.0));

        //Collision handling section

        for i in 0..sphere_count {
            let sphere_pos = glm::vec3(
                sphere_transforms[16 * i + 12],
                sphere_transforms[16 * i + 13],
                sphere_transforms[16 * i + 14]
            );            
            let sphere_radius = 1.0;

            let distance = glm::distance(&sphere_pos, &camera_position);
            let min_distance = sphere_radius + camera_hit_sphere_radius;
            if distance < min_distance {
                let direction = camera_position - sphere_pos;

                camera_position = sphere_pos + glm::normalize(&direction) * min_distance;
            }
        }
        
        //Check for camera collision with the floor
        if camera_position.z < camera_hit_sphere_radius {
            camera_position.z = camera_hit_sphere_radius;
        }
        println!("{:?}", camera_position);

        //Make the light dance around
        uniform_light = glm::normalize(&glm::vec4(4.0 * f32::cos(-0.5 * elapsed_time), 4.0 * f32::sin(-0.5 * elapsed_time), 2.0, 0.0));
        //uniform_light = glm::normalize(&glm::vec4(4.0 * f32::cos(0.5 * elapsed_time), 0.0, 2.0, 0.0));
        shadow_view = glm::look_at(&glm::vec4_to_vec3(&uniform_light), &glm::zero(), &glm::vec3(0.0, 0.0, 1.0));

        //Pre-render phase

        //Create a view matrix from the camera state
        let new_view_matrix = glm::rotation(camera_orientation.y, &glm::vec3(1.0, 0.0, 0.0)) *
                      glm::rotation(camera_orientation.x, &glm::vec3(0.0, 0.0, 1.0)) *
                      glm::translation(&(-camera_position));
        screen_state.update_view(new_view_matrix);

        //Synchronize ui_state before rendering
        ui_state.synchronize();

        //Render
        unsafe {
            //Enable depth test for 3D rendering
            gl::Enable(gl::DEPTH_TEST);

            //Shadow map rendering
            shadow_rendertarget.bind();

            //Teapot render
            gl::UseProgram(shadow_3D);
            glutil::bind_matrix4(shadow_3D, "mvp", &(shadow_projection * shadow_view * teapot_matrix));
            gl::BindVertexArray(teapot_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, teapot_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());

            //Render spheres
            gl::UseProgram(shadow_instanced_3D);
            glutil::bind_matrix4(shadow_instanced_3D, "view_projection", &(shadow_projection * shadow_view));
            sphere_instanced_mesh.draw();

            //Bind common uniforms
            let programs = [complex_3D, complex_instanced_3D];
            for program in &programs {
                glutil::bind_matrix4(*program, "shadow_matrix", &(shadow_projection * shadow_view));
                glutil::bind_vector4(*program, "sun_direction", &uniform_light);
                glutil::bind_int(*program, "shadow_map", ozy::render::TEXTURE_MAP_COUNT as GLint);
                glutil::bind_int(*program, "visualize_normals", visualize_normals as GLint);
                glutil::bind_int(*program, "complex_normals", complex_normals as GLint);
                glutil::bind_int(*program, "outlining", outlining as GLint);
                glutil::bind_vector4(*program, "view_position", &glm::vec4(camera_position.x, camera_position.y, camera_position.z, 1.0));
            }

            //Render into HMD
            match (&xr_session, &mut xr_swapchains, &xr_swapchain_size, &xr_swapchain_images, &mut xr_framewaiter, &mut xr_framestream, &tracking_space) {
                (Some(session), Some(swapchains), Some(sc_size), Some(sc_images), Some(framewaiter), Some(framestream), Some(t_space)) => {
                    let swapchain_size = glm::vec2(sc_size.x as GLint, sc_size.y as GLint);
                    match framewaiter.wait() {
                        Ok(wait_info) => {
                            if let Err(e) = framestream.begin() {
                                println!("{}", e);
                            }
                            let (viewflags, views) = session.locate_views(xr::ViewConfigurationType::PRIMARY_STEREO, wait_info.predicted_display_time, t_space).unwrap();

                            let mut sc_indices = vec![0; views.len()];
                            for i in 0..views.len() {
                                sc_indices[i] = swapchains[i].acquire_image().unwrap();
                                swapchains[i].wait_image(xr::Duration::INFINITE).unwrap();

                                //Bind the framebuffer and bind the swapchain image to its first color attachment
                                let color_texture = sc_images[i][sc_indices[i] as usize];
                                gl::BindFramebuffer(gl::FRAMEBUFFER, xr_swapchain_framebuffer);
                                gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, color_texture, 0);

                                //Bind depth texture to the framebuffer, but create it if it hasn't been yet
                                let depth_index = i * xr_image_count / views.len() + sc_indices[i] as usize;
                                match xr_depth_textures[depth_index] {
                                    Some(tex) => {
                                        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, tex, 0);
                                    }
                                    None => {
                                        let mut width = 0;
                                        let mut height = 0;
                                        gl::BindTexture(gl::TEXTURE_2D, color_texture);
                                        gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
                                        gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);

                                        //Create depth texture
                                        let mut tex = 0;
                                        gl::GenTextures(1, &mut tex);
                                        gl::BindTexture(gl::TEXTURE_2D, tex);

                                        let params = [
                                            (gl::TEXTURE_MAG_FILTER, gl::NEAREST),
                                            (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
                                            (gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE),
                                            (gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE),
                                        ];
                                        glutil::apply_texture_parameters(&params);
                                        gl::TexImage2D(
                                            gl::TEXTURE_2D,
                                            0,
                                            gl::DEPTH_COMPONENT as GLint,
                                            width,
                                            height,
                                            0,
                                            gl::DEPTH_COMPONENT,
                                            gl::FLOAT,
                                            ptr::null()
                                        );

                                        xr_depth_textures[depth_index] = Some(tex);
                                        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, tex, 0);
                                    }
                                }

                                //This is where we would actually do the rendering
                                gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

                                if let Err(e) = swapchains[i].release_image() {
                                    println!("{}", e);
                                }
                            }

                            //End the frame
                            framestream.end(wait_info.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE,
                                &[&xr::CompositionLayerProjection::new()
                                    .space(t_space)
                                    .views(&[
                                        xr::CompositionLayerProjectionView::new()
                                            .pose(views[0].pose)
                                            .fov(views[0].fov)
                                            .sub_image( 
                                                xr::SwapchainSubImage::new()
                                                    .swapchain(&swapchains[0])
                                                    .image_array_index(sc_indices[0])
                                                    .image_rect(xr::Rect2Di {
                                                        offset: xr::Offset2Di { x: 0, y: 0 },
                                                        extent: xr::Extent2Di {width: swapchain_size.x, height: swapchain_size.y}
                                                    })
                                            ),
                                        xr::CompositionLayerProjectionView::new()
                                            .pose(views[1].pose)
                                            .fov(views[1].fov)
                                            .sub_image(
                                                xr::SwapchainSubImage::new()
                                                    .swapchain(&swapchains[1])
                                                    .image_array_index(sc_indices[1])
                                                    .image_rect(xr::Rect2Di {
                                                        offset: xr::Offset2Di { x: 0, y: 0 },
                                                        extent: xr::Extent2Di {width: swapchain_size.x, height: swapchain_size.y}
                                                    })
                                            )
                                    ])
                                ]
                            ).unwrap();
                        }
                        Err(e) => {
                            println!("Error doing framewaiter.wait(): {}", e);
                        }
                    }
                }
                _ => {}
            }

            //Main scene rendering
            default_framebuffer.bind();
            gl::UseProgram(complex_3D);

            let texture_map_names = ["albedo_map", "normal_map", "roughness_map", "shadow_map"];
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                for program in &programs {
                    //Init texture samplers
                    glutil::bind_int(*program, texture_map_names[i], i as GLint);
                }
            }
            gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
            gl::BindTexture(gl::TEXTURE_2D, shadow_rendertarget.texture);

            //Bind textures for the plane
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, plane_mesh.texture_maps[i]);
            }

            //Draw plane mesh
            ozy::glutil::bind_matrix4(complex_3D, "mvp", &(screen_state.get_clipping_from_world() * plane_matrix));
            ozy::glutil::bind_matrix4(complex_3D, "model_matrix", &plane_matrix);
            ozy::glutil::bind_float(complex_3D, "uv_scale", 5.0);
            gl::BindVertexArray(plane_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, plane_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());

            //Bind textures for the teapot
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, teapot_mesh.texture_maps[i]);
            }

            //Bind matrices for teapot
            glutil::bind_matrix4(complex_3D, "mvp", &(screen_state.get_clipping_from_world() * teapot_matrix));
            glutil::bind_matrix4(complex_3D, "model_matrix", &teapot_matrix);
            glutil::bind_float(complex_3D, "uv_scale", 1.0);
            gl::BindVertexArray(teapot_mesh.vao);
            gl::DrawElements(gl::TRIANGLES, teapot_mesh.index_count, gl::UNSIGNED_SHORT, ptr::null());

            //Bind textures for the spheres
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                //Bind textures to said samplers
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, sphere_mesh.texture_maps[i]);
            }

            ozy::glutil::bind_vector4(complex_instanced_3D, "view_position", &glm::vec4(camera_position.x, camera_position.y, camera_position.z, 1.0));
            ozy::glutil::bind_matrix4(complex_instanced_3D, "view_projection", screen_state.get_clipping_from_world());
            ozy::glutil::bind_float(complex_instanced_3D, "uv_scale", 5.0);
            gl::UseProgram(complex_instanced_3D);
            sphere_instanced_mesh.draw();

            //Render 2D elements
            gl::Disable(gl::DEPTH_TEST);
            ui_state.draw(&screen_state);
        }

        window.swap_buffers();
    }
}
