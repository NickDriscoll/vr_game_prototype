#![allow(non_snake_case)]
extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate ozy_engine as ozy;

mod collision;
mod structs;
mod render;
mod xrutil;

use render::{render_main_scene, render_shadows, FragmentFlag, SceneData, ViewData};
use render::{NEAR_DISTANCE, FAR_DISTANCE};

use glfw::{Action, Context, Key, SwapInterval, Window, WindowEvent, WindowHint, WindowMode};
use gl::types::*;
use imgui::{ColorEdit, DrawCmd, EditableColor, FontAtlasRefMut, Slider, TextureId, im_str};
use core::ops::RangeInclusive;
use std::collections::HashMap;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::process::exit;
use std::mem::size_of;
use std::ptr;
use std::os::raw::c_void;
use std::time::Instant;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use ozy::{glutil};
use ozy::glutil::ColorSpace;
use ozy::render::{Framebuffer, InstancedMesh, RenderTarget, ScreenState, SimpleMesh, TextureKeeper};
use ozy::structs::OptionVec;
use ozy::io;
use ozy::routines::uniform_scale;

use crate::collision::*;
use crate::structs::*;

#[cfg(windows)]
use winapi::{um::{winuser::GetWindowDC, wingdi::wglGetCurrentContext}};

//Sets a flag to a value or unsets the flag if it already is the value
fn handle_radio_flag<F: Eq>(current_flag: &mut F, new_flag: F, default: F) {
    if *current_flag != new_flag {
        *current_flag = new_flag;
    } else {
        *current_flag = default;
    }
}

fn reset_player_position(player: &mut Player) {    
    player.tracking_position = glm::vec3(0.0, 0.0, 3.0);
    player.tracking_velocity = glm::zero();
    player.tracked_segment = LineSegment::zero();
    player.last_tracked_segment = LineSegment::zero();
    player.jumps_remaining = Player::MAX_JUMPS;
    player.movement_state = MoveState::Grounded;
}

fn resize_main_window(window: &mut Window, framebuffer: &mut Framebuffer, screen_state: &mut ScreenState, size: glm::TVec2<u32>, pos: (i32, i32), window_mode: WindowMode) {    
    framebuffer.size = (size.x as GLsizei, size.y as GLsizei);
    *screen_state = ScreenState::new(glm::vec2(size.x, size.y), glm::identity(), glm::half_pi(), NEAR_DISTANCE, FAR_DISTANCE);
    window.set_monitor(window_mode, pos.0, pos.1, size.x, size.y, Some(144));
}

fn clamp<T: PartialOrd>(x: T, min: T, max: T) -> T {
    if x < min { min }
    else if x > max { max }
    else { x }
}

fn write_matrix_to_buffer(buffer: &mut [f32], index: usize, matrix: glm::TMat4<f32>) {    
    for k in 0..16 {
        buffer[16 * index + k] = matrix[k];
    }
}

fn main() {
    //Initialize the configuration data
    let config = {
        match Configuration::from_file(Configuration::CONFIG_FILEPATH) {
            Some(cfg) => { cfg }
            None => {
                let mut int_options = HashMap::new();
                let mut string_options = HashMap::new();
                int_options.insert(String::from(Configuration::WINDOWED_WIDTH), 1280);
                int_options.insert(String::from(Configuration::WINDOWED_HEIGHT), 720);
                string_options.insert(String::from(Configuration::LEVEL_NAME), String::from("testmap"));
                let c = Configuration {
                    int_options,
                    string_options
                };
                c.to_file(Configuration::CONFIG_FILEPATH);
                c
            }
        }
    };

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
            Ok(set) => { Some(set) }
            Err(e) => {
                println!("Extention enumerations error: {}", e);
                None
            }
        };

        //Make sure the local OpenXR implementation supports OpenGL
        if let Some(set) = &extension_set {
            if !set.khr_opengl_enable {
                println!("OpenXR implementation does not support OpenGL!");
                exit(-1);
            }
        } 

        if let Ok(layer_properties) = openxr_entry.enumerate_layers() {
            for layer in layer_properties.iter() {
                println!("{}", layer.layer_name);
            }
        }
        
        //Create the instance
        let mut instance = None;

        if let Some(ext_set) = &extension_set {
            match openxr_entry.create_instance(&app_info, ext_set, &[]) {
                Ok(inst) => { instance = Some(inst) }
                Err(e) => { 
                    println!("Error creating OpenXR instance: {}", e);
                    instance = None;
                }
            }
        }
        
        instance
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

    //Create the actionset
    let xr_controller_actionset = match &xr_instance {
        Some(inst) => {
            match inst.create_action_set("controllers", "Controllers", 1) {
                Ok(set) => { Some(set) }
                Err(e) => {
                    println!("Error creating XrActionSet: {}", e);
                    None
                }
            }
        }
        None => { None }
    };

    //Create the paths to appropriate equipment
    let left_grip_pose_path = xrutil::make_path(&xr_instance, xrutil::LEFT_GRIP_POSE);
    let left_aim_pose_path = xrutil::make_path(&xr_instance, xrutil::LEFT_AIM_POSE);
    let left_trigger_float_path = xrutil::make_path(&xr_instance, xrutil::LEFT_TRIGGER_FLOAT);
    let right_trigger_float_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRIGGER_FLOAT);
    let right_grip_pose_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_GRIP_POSE);
    let right_aim_pose_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_AIM_POSE);
    let right_trackpad_force_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRACKPAD_FORCE);
    let right_trackpad_click_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRACKPAD_CLICK);
    let left_stick_vector_path = xrutil::make_path(&xr_instance, xrutil::LEFT_STICK_VECTOR2);
    let left_trackpad_vector_path = xrutil::make_path(&xr_instance, xrutil::LEFT_TRACKPAD_VECTOR2);
    let right_a_button_bool_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_A_BUTTON_BOOL);

    //Create the hand subaction paths
    let left_hand_subaction_path = xrutil::make_path(&xr_instance, xr::USER_HAND_LEFT);
    let right_hand_subaction_path = xrutil::make_path(&xr_instance, xr::USER_HAND_RIGHT);

    //Create the XrActions
    let left_hand_pose_action = xrutil::make_action(&left_hand_subaction_path, &xr_controller_actionset, "left_hand_pose", "Left hand pose");
    let left_hand_aim_action = xrutil::make_action::<xr::Posef>(&left_hand_subaction_path, &xr_controller_actionset, "left_hand_aim", "Left hand aim");
    let left_trigger_action = xrutil::make_action::<f32>(&left_hand_subaction_path, &xr_controller_actionset, "left_hand_trigger", "Left hand trigger");
    let right_trigger_action = xrutil::make_action::<f32>(&right_hand_subaction_path, &xr_controller_actionset, "right_hand_trigger", "Right hand trigger");
    let right_hand_grip_action = xrutil::make_action(&right_hand_subaction_path, &xr_controller_actionset, "right_hand_pose", "Right hand pose");
    let right_hand_aim_action = xrutil::make_action(&right_hand_subaction_path, &xr_controller_actionset, "right_hand_aim", "Right hand aim");
    let go_home_action = xrutil::make_action::<bool>(&right_hand_subaction_path, &xr_controller_actionset, "item_menu", "Interact with item menu");
    let player_move_action = xrutil::make_action::<xr::Vector2f>(&left_hand_subaction_path, &xr_controller_actionset, "player_move", "Player movement");

    //Suggest interaction profile bindings
    match (&xr_instance,
           &left_hand_pose_action,
           &left_hand_aim_action,
           &left_trigger_action,
           &right_trigger_action,
           &right_hand_grip_action,
           &player_move_action,
           &left_grip_pose_path,
           &left_aim_pose_path,
           &left_trigger_float_path,
           &right_trigger_float_path,
           &right_grip_pose_path,
           &left_stick_vector_path,
           &left_trackpad_vector_path,
           &right_trackpad_force_path,
           &go_home_action,
           &right_hand_aim_action,
           &right_aim_pose_path,
           &right_trackpad_click_path,
           &right_a_button_bool_path) {
        (Some(inst),
         Some(l_grip_action),
         Some(l_aim_action),
         Some(l_trigger_action),
         Some(r_trigger_action),
         Some(r_action),
         Some(move_action),
         Some(l_grip_path),
         Some(l_aim_path),
         Some(l_trigger_path),
         Some(r_trigger_path),
         Some(r_path),
         Some(l_stick_path),
         Some(l_trackpad_path),
         Some(r_trackpad_force),
         Some(i_menu_action),
         Some(r_aim_action),
         Some(r_aim_path),
         Some(r_track_click_path),
         Some(r_a_button_path)) => {
            //Valve Index
            let bindings = [
                xr::Binding::new(l_grip_action, *l_grip_path),
                xr::Binding::new(l_aim_action, *l_aim_path),
                xr::Binding::new(r_aim_action, *r_aim_path),
                xr::Binding::new(l_trigger_action, *l_trigger_path),
                xr::Binding::new(r_trigger_action, *r_trigger_path),
                xr::Binding::new(r_action, *r_path),
                xr::Binding::new(move_action, *l_stick_path),
                xr::Binding::new(i_menu_action, *r_trackpad_force)
            ];
            xrutil::suggest_bindings(inst, xrutil::VALVE_INDEX_INTERACTION_PROFILE, &bindings);

            //HTC Vive
            let bindings = [
                xr::Binding::new(l_grip_action, *l_grip_path),
                xr::Binding::new(l_aim_action, *l_aim_path),
                xr::Binding::new(r_aim_action, *r_aim_path),
                xr::Binding::new(l_trigger_action, *l_trigger_path),
                xr::Binding::new(r_trigger_action, *r_trigger_path),
                xr::Binding::new(r_action, *r_path),
                xr::Binding::new(move_action, *l_trackpad_path),                   
                xr::Binding::new(i_menu_action, *r_track_click_path)
            ];
            xrutil::suggest_bindings(inst, xrutil::HTC_VIVE_INTERACTION_PROFILE, &bindings);

            //Oculus Touch
            let bindings = [
                xr::Binding::new(l_grip_action, *l_grip_path),
                xr::Binding::new(l_aim_action, *l_aim_path),
                xr::Binding::new(r_aim_action, *r_aim_path),
                xr::Binding::new(l_trigger_action, *l_trigger_path),
                xr::Binding::new(r_trigger_action, *r_trigger_path),
                xr::Binding::new(r_action, *r_path),
                xr::Binding::new(move_action, *l_stick_path),
                xr::Binding::new(i_menu_action, *r_a_button_path)
            ];
            xrutil::suggest_bindings(inst, xrutil::OCULUS_TOUCH_INTERACTION_PROFILE, &bindings);
        }
        _ => {}
    }

    //Initialize glfw
    let mut glfw = match glfw::init(glfw::FAIL_ON_ERRORS) {
        Ok(g) => { g }
        Err(e) => { panic!("{}", e) }
    };
    
    //Ask for an OpenGL version based on what OpenXR requests. Default to 4.3
    match xr_graphics_reqs {
        Some(r) => { glfw.window_hint(glfw::WindowHint::ContextVersion(r.min_api_version_supported.major() as u32, r.min_api_version_supported.minor() as u32)); }
        None => { glfw.window_hint(glfw::WindowHint::ContextVersion(4, 3)); }
    }

    //Initialize screen state
    let mut screen_state = {
        let window_size = get_window_size(&config);
        let fov_radians = glm::half_pi();
        ScreenState::new(window_size, glm::identity(), fov_radians, NEAR_DISTANCE, FAR_DISTANCE)
    };

    //Create the window
	glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
	glfw.window_hint(WindowHint::Samples(Some(render::MSAA_SAMPLES)));
    let (mut window, events) = match glfw.create_window(screen_state.get_window_size().x, screen_state.get_window_size().y, "THCATO", glfw::WindowMode::Windowed) {
        Some(stuff) => { stuff }
        None => {
            panic!("Unable to create a window!");
        }
    };
    window.set_resizable(false);
    window.set_key_polling(true);
    window.set_mouse_button_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_scroll_polling(true);
    window.set_framebuffer_size_polling(true);

    //Load OpenGL function pointers
    gl::load_with(|symbol| window.get_proc_address(symbol));

    //OpenGL static configuration
	unsafe {
        gl::Enable(gl::CULL_FACE);										//Enable face culling
        gl::DepthFunc(gl::LEQUAL);										//Pass the fragment with the smallest z-value.
		gl::Enable(gl::FRAMEBUFFER_SRGB); 								//Enable automatic linear->SRGB space conversion
        gl::Enable(gl::BLEND);											//Enable alpha blending
        gl::Enable(gl::MULTISAMPLE);                                    //Enable MSAA
		gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);			//Set blend func to (Cs * alpha + Cd * (1.0 - alpha))
        gl::ClearColor(0.26, 0.4, 0.46, 1.0);							//Set the clear color

		#[cfg(gloutput)]
		{
			gl::Enable(gl::DEBUG_OUTPUT);									                                    //Enable verbose debug output
			gl::Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);						                                    //Synchronously call the debug callback function
			gl::DebugMessageCallback(ozy::glutil::gl_debug_callback, ptr::null());		                        //Register the debug callback
			gl::DebugMessageControl(gl::DONT_CARE, gl::DONT_CARE, gl::DONT_CARE, 0, ptr::null(), gl::TRUE);
		}
    }
    //glfw.set_swap_interval(SwapInterval::None);

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
                println!("Unable to attach action sets: {}", e);
            }
        }
        _ => {}
    }

    //Define tracking space with z-up instead of the default y-up
    let space_pose = {
        let quat = glm::quat_rotation(&glm::vec3(0.0, 0.0, 1.0), &glm::vec3(0.0, 1.0, 0.0));
        xr::Posef {
            orientation: xr::Quaternionf {
                x: quat.coords.x,
                y: quat.coords.y,
                z: quat.coords.z,
                w: quat.coords.w,
            },
            position: xr::Vector3f {
                x: 0.0,
                y: 0.0,
                z: 0.0
            }
        }
    };
    let tracking_space = xrutil::make_reference_space(&xr_session, xr::ReferenceSpaceType::STAGE, space_pose);           //Create tracking space
    let view_space = xrutil::make_reference_space(&xr_session, xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY);       //Create view space
    
    let left_hand_grip_space = xrutil::make_actionspace(&xr_session, left_hand_subaction_path, &left_hand_pose_action, space_pose); //Create left hand grip space
    let left_hand_aim_space = xrutil::make_actionspace(&xr_session, left_hand_subaction_path, &left_hand_aim_action, space_pose); //Create left hand aim space
    let right_hand_grip_space = xrutil::make_actionspace(&xr_session, right_hand_subaction_path, &right_hand_grip_action, space_pose); //Create right hand grip space
    let right_hand_aim_space = xrutil::make_actionspace(&xr_session, right_hand_subaction_path, &right_hand_aim_action, space_pose); //Create right hand aim space

    //Create swapchains
    let mut xr_swapchains = match (&xr_session, &xr_viewconfiguration_views) {
        (Some(session), Some(viewconfig_views)) => {
            let mut failed = false;
            let mut scs = Vec::with_capacity(viewconfig_views.len());
            for viewconfig in viewconfig_views {
                let create_info = xr::SwapchainCreateInfo {
                    create_flags: xr::SwapchainCreateFlags::EMPTY,
                    usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
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

    //Create swapchain FBO
    let xr_swapchain_framebuffer = unsafe {
        let mut p = 0;
        gl::GenFramebuffers(1, &mut p);
        p
    };

    //MSAA rendertarget which will have the scene rendered into it before blitting to the actual HMD swapchain image
    //This gets around the fact that SteamVR refuses to allocate MSAA images :) :) :)
    let xr_swapchain_rendertarget = match xr_swapchain_size {
        Some(size) => unsafe { Some(RenderTarget::new_multisampled((size.x as GLint, size.y as GLint), render::MSAA_SAMPLES as GLint)) }
        None => { None }
    };

    let xr_swapchain_images = match &xr_swapchains {
        Some(chains) => {
            let mut failed = false;
            let mut image_arr = Vec::with_capacity(chains.len());
            for chain in chains.iter() {
                match chain.enumerate_images() {
                    Ok(images) => {
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

    //Compile shader programs
    let complex_3D = unsafe { glutil::compile_program_from_files("shaders/mapped.vert", "shaders/mapped.frag") };
    let complex_instanced_3D = unsafe { glutil::compile_program_from_files("shaders/mapped_instanced.vert", "shaders/mapped.frag") };
    let shadow_3D = unsafe { glutil::compile_program_from_files("shaders/shadow.vert", "shaders/shadow.frag") };
    let shadow_instanced_3D = unsafe { glutil::compile_program_from_files("shaders/shadow_instanced.vert", "shaders/shadow.frag") };
    let skybox_program = unsafe { glutil::compile_program_from_files("shaders/skybox.vert", "shaders/skybox.frag") };

    //Special program for rendering Dear ImGui
    let imgui_program = unsafe { glutil::compile_program_from_files("shaders/ui/imgui.vert", "shaders/ui/imgui.frag") };
    
    //Initialize default framebuffer
    let mut default_framebuffer = Framebuffer {
        name: 0,
        size: (screen_state.get_window_size().x as GLsizei, screen_state.get_window_size().y as GLsizei),
        clear_flags: gl::DEPTH_BUFFER_BIT | gl::COLOR_BUFFER_BIT,
        cull_face: gl::BACK
    };

    let mut mouselook_enabled = false;
    let mut camera_position = glm::vec3(0.0, -8.0, 5.5);
    let mut last_camera_position = camera_position;
    let mut camera_input: glm::TVec4<f32> = glm::zero();             //This is a unit vector  in view space that represents the input camera movement vector
    let mut camera_orientation = glm::vec2(0.0, -glm::half_pi::<f32>() * 0.6);
    let mut camera_speed = 5.0;
    let camera_hit_sphere_radius = 0.5;

    //Initialize shadow data
    let mut shadow_view;
    let shadow_proj_size = 30.0;
    let shadow_projection = glm::ortho(-shadow_proj_size * 2.0, shadow_proj_size * 2.0, -shadow_proj_size * 2.0, shadow_proj_size * 2.0, 3.0 * -shadow_proj_size, 2.0 * shadow_proj_size);
    let shadow_size = 4096 * 4;
    let shadow_rendertarget = unsafe { RenderTarget::new_shadow((shadow_size, shadow_size)) };

    //Initialize scene data struct
    let mut scene_data = SceneData::new([complex_3D, complex_instanced_3D, skybox_program, shadow_3D, shadow_instanced_3D], shadow_rendertarget.texture);

    //Initialize texture caching struct
    let mut texture_keeper = TextureKeeper::new();
    let default_tex_params = [  
        (gl::TEXTURE_WRAP_S, gl::REPEAT),
	    (gl::TEXTURE_WRAP_T, gl::REPEAT),
	    (gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR),
	    (gl::TEXTURE_MAG_FILTER, gl::LINEAR_MIPMAP_LINEAR)
    ];

    //Player state
    let mut player = Player {
        tracking_position: glm::vec3(0.0, 0.0, 1.0),
        tracking_velocity: glm::zero(),
        tracked_segment: LineSegment::zero(),
        last_tracked_segment: LineSegment::zero(),
        movement_state: MoveState::Grounded,
        radius: 0.15,
        jumps_remaining: Player::MAX_JUMPS,
        was_holding_left_trigger: false
    };

    //Water gun state
    const MAX_WATER_PRESSURE: f32 = 30.0;
    const MAX_WATER_REMAINING: f32 = 75.0;
    let mut water_gun_force = glm::zero();
    let mut infinite_ammo = false;
    let mut remaining_water = MAX_WATER_REMAINING;
    let mut water_pillar_scale = glm::vec3(1.0, 1.0, 1.0);
    let water_cylinder_mesh = SimpleMesh::from_ozy("models/water_cylinder.ozy", &mut texture_keeper, &default_tex_params);
    let water_cylinder_entity_index = scene_data.push_single_entity(water_cylinder_mesh);
    
    //Matrices for relating tracking space and world space
    let mut world_from_tracking = glm::identity();
    let mut tracking_from_world = glm::affine_inverse(world_from_tracking);

    //OptionVec to hold all AABBs used for collision
    let mut collision_aabbs: OptionVec<AABB> = OptionVec::new();

    let mut screen_space_mouse = glm::zero();

    //Creating Dear ImGui context
    let mut imgui_context = imgui::Context::create();
    imgui_context.style_mut().use_dark_colors();
    {
        let io = imgui_context.io_mut();
        io.display_size[0] = screen_state.get_window_size().x as f32;
        io.display_size[1] = screen_state.get_window_size().y as f32;
    }
    let mut do_imgui = false;

    //Create and upload Dear IMGUI font atlas
    match imgui_context.fonts() {
        FontAtlasRefMut::Owned(atlas) => unsafe {
            let mut tex = 0;
            let font_atlas = atlas.build_alpha8_texture();
            
            let font_atlas_params = [                
                (gl::TEXTURE_WRAP_S, gl::REPEAT),
                (gl::TEXTURE_WRAP_T, gl::REPEAT),
                (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
                (gl::TEXTURE_MAG_FILTER, gl::NEAREST)
            ];

            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);            
            glutil::apply_texture_parameters(&font_atlas_params);
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RED as GLsizei, font_atlas.width as GLsizei, font_atlas.height as GLsizei, 0, gl::RED, gl::UNSIGNED_BYTE, font_atlas.data.as_ptr() as _);
            atlas.tex_id = TextureId::new(tex as usize);
        }
        FontAtlasRefMut::Shared(_) => {
            panic!("Not dealing with this case.");
        }
    };
    
    //Load terrain data
    let terrain;
    {
        let terrain_name = match config.string_options.get(Configuration::LEVEL_NAME) {
            Some(name) => { name }
            None => { "testmap" }
        };
        terrain = Terrain::from_ozt(&format!("models/{}.ozt", terrain_name));

        //Load the scene data from the level file
        match File::open(&format!("maps/{}.lvl", terrain_name)) {
            Ok(mut file) => {
                loop {
                    //Read ozy name
                    let ozy_name = match io::read_pascal_strings(&mut file, 1) {
                        Ok(v) => { v[0].clone() }
                        Err(e) => {
                            //We expect this call to eventually return EOF
                            if e.kind() == ErrorKind::UnexpectedEof {
                                break;
                            }
                            panic!("Error reading from level file: {}", e);                            
                        }
                    };

                    //Read number of matrices
                    let matrices_count = match io::read_u32(&mut file) {
                        Ok(count) => { count } 
                        Err(e) => {
                            panic!("Error reading from level file: {}", e);
                        }
                    };
                    let matrix_floats = match io::read_f32_data(&mut file, matrices_count as usize * 16) {
                        Ok(floats) => { floats }
                        Err(e) => {
                            panic!("Error reading from level file: {}", e);
                        }
                    };
                    let mesh = SimpleMesh::from_ozy(&format!("models/{}", ozy_name), &mut texture_keeper, &default_tex_params);

                    if matrices_count == 1 {
                        let idx = scene_data.push_single_entity(mesh);
                        scene_data.get_single_entity(idx).unwrap().model_matrix = glm::mat4(
                            matrix_floats[0], matrix_floats[4], matrix_floats[8], matrix_floats[12], 
                            matrix_floats[1], matrix_floats[5], matrix_floats[9], matrix_floats[13], 
                            matrix_floats[2], matrix_floats[6], matrix_floats[10], matrix_floats[14], 
                            matrix_floats[3], matrix_floats[7], matrix_floats[11], matrix_floats[15]
                        );
                    } else {
                        let in_mesh = unsafe { InstancedMesh::from_simplemesh(&mesh, matrices_count as usize, render::INSTANCED_ATTRIBUTE) };
                        let idx = scene_data.push_instanced_entity(in_mesh);
                        scene_data.get_instanced_entity(idx).unwrap().mesh.update_buffer(&matrix_floats);
                    }
                }
                
            }
            Err(e) => {
                panic!("Couldn't open level file: {}", e);
            }
        }
    }

    //Create dragon
    let mut dragon_position = glm::vec3(19.209993, 0.5290663, 0.0);
    let dragon_mesh = SimpleMesh::from_ozy("models/dragon.ozy", &mut texture_keeper, &default_tex_params);
    let dragon_entity_index = scene_data.push_single_entity(dragon_mesh);

    //Create staircase
    let cube_count = 2048;
    let mut cube_transform_buffer = vec![0.0; cube_count * 16];
    let cube_entity_index = unsafe {
        let mesh = SimpleMesh::from_ozy("models/cube.ozy", &mut texture_keeper, &default_tex_params);
        let mesh = InstancedMesh::from_simplemesh(&mesh, cube_count, render::INSTANCED_ATTRIBUTE);

        for i in 0..cube_count {
            let position = glm::vec4(-20.0, i as f32 * 10.0, i as f32 * 5.0, 1.0);
            let matrix = glm::translation(&glm::vec4_to_vec3(&position)) * uniform_scale(3.0);
            write_matrix_to_buffer(&mut cube_transform_buffer, i, matrix);

            let aabb = AABB {
                position,
                width: 3.0,
                depth: 3.0,
                height: 3.0,
            };
            collision_aabbs.insert(aabb);
        }

        scene_data.push_instanced_entity(mesh)
    };
    scene_data.get_instanced_entity(cube_entity_index).unwrap().mesh.update_buffer(&cube_transform_buffer);

    //Create the cube that will be user to render the skybox
	scene_data.skybox_vao = ozy::prims::skybox_cube_vao();

	//Create the skybox cubemap
	scene_data.skybox_cubemap = unsafe {
		let name = "siege";
		let paths = [
			&format!("skyboxes/{}_rt.tga", name),		//Right side
			&format!("skyboxes/{}_lf.tga", name),		//Left side
			&format!("skyboxes/{}_up.tga", name),		//Up side
			&format!("skyboxes/{}_dn.tga", name),		//Down side
			&format!("skyboxes/{}_bk.tga", name),		//Back side
			&format!("skyboxes/{}_ft.tga", name)		//Front side
		];

		let mut cubemap = 0;
		gl::GenTextures(1, &mut cubemap);
		gl::BindTexture(gl::TEXTURE_CUBE_MAP, cubemap);
		gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
		gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
		gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_WRAP_R, gl::CLAMP_TO_EDGE as i32);
		gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
		gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);

		//Place each piece of the skybox on the correct face
		for i in 0..6 {
			let image_data = glutil::image_data_from_path(paths[i], ColorSpace::Gamma);
			gl::TexImage2D(gl::TEXTURE_CUBE_MAP_POSITIVE_X + i as u32,
						   0,
						   image_data.internal_format as i32,
						   image_data.width as i32,
						   image_data.height as i32,
						   0,
						   image_data.format,
						   gl::UNSIGNED_BYTE,
				  		   &image_data.data[0] as *const u8 as *const c_void);
		}
		cubemap
	};

    //Create controller entities
    let mut left_wand_entity_index = 0;
    let mut right_wand_entity_index = 0;
    if let Some(_) = &xr_instance {
        let wand_mesh = SimpleMesh::from_ozy("models/wand.ozy", &mut texture_keeper, &default_tex_params);
        left_wand_entity_index = scene_data.push_single_entity(wand_mesh.clone());
        right_wand_entity_index = scene_data.push_single_entity(wand_mesh);
    }

    let mut is_fullscreen = false;
    let mut wireframe = false;
    let mut true_wireframe = false;
    let mut placing = false;
    let mut hmd_pov = false;
    if let Some(_) = &xr_instance { hmd_pov = true; }

    let mut frame_count = 0;
    let mut last_frame_instant = Instant::now();
    let mut last_xr_render_time = xr::Time::from_nanos(0);
    let mut elapsed_time = 0.0;

    //Main loop
    while !window.should_close() {
        let imgui_io = imgui_context.io_mut();
        //Compute the number of seconds since the start of the last frame (i.e at 60fps, delta_time ~= 0.016667)
        let delta_time = {
			let frame_instant = Instant::now();
			let dur = frame_instant.duration_since(last_frame_instant);
			last_frame_instant = frame_instant;
			dur.as_secs_f32()
        };
        elapsed_time += delta_time;
        frame_count += 1;
        imgui_io.delta_time = delta_time;
        let framerate = imgui_io.framerate;

        //Sync OpenXR actions
        if let (Some(session), Some(controller_actionset)) = (&xr_session, &xr_controller_actionset) {
            if let Err(e) = session.sync_actions(&[xr::ActiveActionSet::new(controller_actionset)]) {
                println!("Unable to sync actions: {}", e);
            }
        }

        //Get action states
        let left_stick_state = xrutil::get_actionstate(&xr_session, &player_move_action);
        let left_trigger_state = xrutil::get_actionstate(&xr_session, &left_trigger_action);
        let right_trigger_state = xrutil::get_actionstate(&xr_session, &right_trigger_action);
        let right_trackpad_force_state = xrutil::get_actionstate(&xr_session, &go_home_action);

        //Emergency escape button
        if let Some(state) = right_trackpad_force_state {
            if state.changed_since_last_sync && state.current_state {
                reset_player_position(&mut player);
            }
        }

        //Poll window events and handle them
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Close => { window.set_should_close(true); }
                WindowEvent::Key(key, _, Action::Press, _) => {
                    match key {
                        Key::Escape => { do_imgui = !do_imgui; }
                        Key::Space => { placing = !placing; }
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
                        Key::Q => {
                            camera_input.y -= 1.0;
                        }
                        Key::E => {
                            camera_input.y += 1.0;
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
                        Key::Q => {
                            camera_input.y += 1.0;
                        }
                        Key::E => {
                            camera_input.y -= 1.0;
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
                    match action {
                        Action::Press => {
                            imgui_io.mouse_down[0] = true;
                        }
                        Action::Release => {
                            imgui_io.mouse_down[0] = false;
                        }
                        Action::Repeat => {}
                    }
                }
                WindowEvent::MouseButton(glfw::MouseButtonRight, glfw::Action::Press, ..) => {
                    imgui_io.mouse_down[1] = true;
                }
                WindowEvent::MouseButton(glfw::MouseButtonRight, glfw::Action::Release, ..) => {
                    imgui_io.mouse_down[1] = false;
                    if mouselook_enabled {
                        window.set_cursor_mode(glfw::CursorMode::Normal);
                    } else {
                        window.set_cursor_mode(glfw::CursorMode::Hidden);
                    }
                    mouselook_enabled = !mouselook_enabled;
                }
                WindowEvent::CursorPos(x, y) => {
                    imgui_io.mouse_pos = [x as f32, y as f32];
                    screen_space_mouse = glm::vec2(x as f32, y as f32);
                    if mouselook_enabled {
                        const CAMERA_SENSITIVITY_DAMPENING: f32 = 0.002;
                        let offset = glm::vec2(screen_space_mouse.x as f32 - screen_state.get_window_size().x as f32 / 2.0, screen_space_mouse.y as f32 - screen_state.get_window_size().y as f32 / 2.0);
                        camera_orientation += offset * CAMERA_SENSITIVITY_DAMPENING;
                        if camera_orientation.y < -glm::pi::<f32>() {
                            camera_orientation.y = -glm::pi::<f32>();
                        } else if camera_orientation.y > 0.0 {
                            camera_orientation.y = 0.0;
                        }
                    }
                }
                WindowEvent::Scroll(x, y) => {
                    imgui_io.mouse_wheel_h = x as f32;
                    imgui_io.mouse_wheel = y as f32;
                }
                WindowEvent::FramebufferSize(width, height) => {
                    imgui_io.display_size[0] = width as f32;
                    imgui_io.display_size[1] = height as f32;
                }
                _ => {  }
            }
        }
        drop(imgui_io);
        
        //Begin drawing imgui frame
        let imgui_ui = imgui_context.frame();

        //Gravity the player
        const GRAVITY_VELOCITY_CAP: f32 = 10.0;
        const ACCELERATION_GRAVITY: f32 = 20.0;        //20.0 m/s^2
        if player.movement_state != MoveState::Grounded {
            player.tracking_velocity.z -= ACCELERATION_GRAVITY * delta_time;
            if player.tracking_velocity.z > GRAVITY_VELOCITY_CAP {
                player.tracking_velocity.z = GRAVITY_VELOCITY_CAP;
            }
        }

        //Calculate the velocity of tracking space
        {
            const MOVEMENT_SPEED: f32 = 5.0;
            const DEADZONE_MAGNITUDE: f32 = 0.1;
            if let Some(stick_state) = &left_stick_state {
                if stick_state.changed_since_last_sync && player.movement_state != MoveState::Sliding {                            
                    if let Some(pose) = xrutil::locate_space(&left_hand_aim_space, &tracking_space, stick_state.last_change_time) {
                        let hand_space_vec = glm::vec4(stick_state.current_state.x, stick_state.current_state.y, 0.0, 0.0);
                        let magnitude = glm::length(&hand_space_vec);
                        if magnitude < DEADZONE_MAGNITUDE {
                            player.tracking_velocity.x = 0.0;
                            player.tracking_velocity.y = 0.0;
                        } else {
                            //Explicit check for zero to avoid divide-by-zero in normalize
                            if hand_space_vec != glm::zero() {
                                //World space untreated vector
                                let untreated = xrutil::pose_to_mat4(&pose, &world_from_tracking) * hand_space_vec;
                                let ugh = glm::normalize(&glm::vec3(untreated.x, untreated.y, 0.0)) * MOVEMENT_SPEED * magnitude;
                                player.tracking_velocity = glm::vec3(ugh.x, ugh.y, player.tracking_velocity.z);
                            }
                        }
                    }
                }
            }

            if let Some(state) = &left_trigger_state {
                let holding = state.current_state == 1.0;

                if holding && !player.was_holding_left_trigger && player.jumps_remaining > 0 {
                    player.tracking_velocity = glm::vec3(player.tracking_velocity.x, player.tracking_velocity.y, 10.0);

                    set_player_falling(&mut player);
                } else if state.current_state < 1.0 && player.was_holding_left_trigger && player.tracking_velocity.z > 0.0 {
                    player.tracking_velocity.z /= 2.0;
                }

                player.was_holding_left_trigger = holding;
            }
        }

        //Do water gun stuff
        {
            //Calculate the force of shooting the water gun
            if let (Some(state), Some(pose)) = (right_trigger_state, xrutil::locate_space(&right_hand_aim_space, &tracking_space, last_xr_render_time)) {

                let hand_transform = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                let hand_space_vec = glm::vec4(0.0, 1.0, 0.0, 0.0);
                let world_space_vec = hand_transform * hand_space_vec;
                let hand_origin = hand_transform * glm::vec4(0.0, 0.0, 0.0, 1.0);

                //Calculate water gun force vector
                water_gun_force = glm::vec4_to_vec3(&(-state.current_state * world_space_vec));

                if state.current_state > 0.0 {
                    //Calculate scale of water pillar
                    if let Some(point) = ray_hit_terrain(&terrain, &hand_origin, &world_space_vec) {
                        water_pillar_scale.y = glm::length(&(point - hand_origin)); 
                    }

                    if player.movement_state != MoveState::Falling {
                        set_player_falling(&mut player);
                    }
                }
            }

            if water_gun_force != glm::zero() && remaining_water > 0.0 {
                let update_force = water_gun_force * delta_time * MAX_WATER_PRESSURE;
                if !infinite_ammo {
                    remaining_water -= glm::length(&update_force);
                }
                let xz_scale = remaining_water / MAX_WATER_REMAINING;
                water_pillar_scale.x = xz_scale;
                water_pillar_scale.z = xz_scale;
                player.tracking_velocity += update_force;
                if let Some(entity) = scene_data.get_single_entity(water_cylinder_entity_index) {
                    entity.visible = true;
                }
            } else {
                if let Some(entity) = scene_data.get_single_entity(water_cylinder_entity_index) {
                    entity.visible = false;
                }
            }
            if player.movement_state != MoveState::Falling {
                remaining_water = MAX_WATER_REMAINING;
            }
        }

        //If the user is controlling the camera, force the mouse cursor into the center of the screen
        if mouselook_enabled {
            window.set_cursor_pos(screen_state.get_window_size().x as f64 / 2.0, screen_state.get_window_size().y as f64 / 2.0);
        }

        let camera_velocity = camera_speed * glm::vec4_to_vec3(&(glm::affine_inverse(*screen_state.get_view_from_world()) * camera_input));
        camera_position += camera_velocity * delta_time;

        //Place dragon at clicking position
        if placing {
            let fovx_radians = 2.0 * f32::atan(f32::tan(screen_state.get_fov_radians() / 2.0) * screen_state.get_aspect_ratio());
            let max_coords = glm::vec4(
                NEAR_DISTANCE * f32::tan(fovx_radians / 2.0),
                NEAR_DISTANCE * f32::tan(screen_state.get_fov_radians() / 2.0),
                -NEAR_DISTANCE,
                1.0
            );
            let normalized_coords = glm::vec4(
                screen_space_mouse.x * 2.0 / screen_state.get_window_size().x as f32 - 1.0,
                -screen_space_mouse.y * 2.0 / screen_state.get_window_size().y as f32 + 1.0,
                1.0,
                1.0
            );
            let view_space_mouse = glm::matrix_comp_mult(&normalized_coords, &max_coords);
            let world_space_mouse = screen_state.get_world_from_view() * view_space_mouse;

            let ray_origin = glm::vec4(camera_position.x, camera_position.y, camera_position.z, 1.0);
            let mouse_ray_dir = glm::normalize(&(world_space_mouse - ray_origin));

            //Update dragon's position if the ray hit
            if let Some(point) = ray_hit_terrain(&terrain, &ray_origin, &mouse_ray_dir) {
                dragon_position = glm::vec4_to_vec3(&point);
            }
        }

        //Spin the dragon
        if let Some(entity) = scene_data.get_single_entity(dragon_entity_index) {
            entity.model_matrix = glm::translation(&dragon_position) * glm::rotation(elapsed_time, &glm::vec3(0.0, 0.0, 1.0));
        }

        //Update the water gun's pillar of water
        if let Some(entity) = scene_data.get_single_entity(water_cylinder_entity_index) {
            entity.uv_offset += glm::vec2(0.0, 5.0) * delta_time;
            entity.uv_scale.y = water_pillar_scale.y;
        }

        //Update tracking space location
        player.tracking_position += player.tracking_velocity * delta_time;
        world_from_tracking = glm::translation(&player.tracking_position);

        //Collision handling section

        //Check for camera collision with aabbs
        for opt_aabb in collision_aabbs.iter() {
            if let Some(aabb) = opt_aabb {
                let closest_point = glm::vec3(
                    clamp(camera_position.x, aabb.position.x - aabb.width, aabb.position.x + aabb.width),
                    clamp(camera_position.y, aabb.position.y - aabb.depth, aabb.position.y + aabb.depth),
                    clamp(camera_position.z, aabb.position.z, aabb.position.z + aabb.height * 2.0)
                );

                let distance = glm::distance(&camera_position, &closest_point);
                if distance > 0.0 && distance < camera_hit_sphere_radius {
                    let vec = glm::normalize(&(camera_position - closest_point));
                    camera_position += (camera_hit_sphere_radius - distance) * vec;
                } else if distance == 0.0 {
                    //Prevent the camera from breaking into AABBs by moving fast enough
                    let segment = LineSegment {
                        p0: glm::vec4(last_camera_position.x, last_camera_position.y, last_camera_position.z, 1.0),
                        p1: glm::vec4(camera_position.x, camera_position.y, camera_position.z, 1.0),
                    };

                    //Array of the planes that define the AABB
                    let planes = [
                        Plane::new(aabb.position + glm::vec4(aabb.width, 0.0, 0.0, 0.0), glm::vec4(1.0, 0.0, 0.0, 0.0)),
                        Plane::new(aabb.position + glm::vec4(-aabb.width, 0.0, 0.0, 0.0), glm::vec4(-1.0, 0.0, 0.0, 0.0)),
                        Plane::new(aabb.position + glm::vec4(0.0, aabb.depth, 0.0, 0.0), glm::vec4(0.0, 1.0, 0.0, 0.0)),
                        Plane::new(aabb.position + glm::vec4(0.0, -aabb.depth, 0.0, 0.0), glm::vec4(0.0, -1.0, 0.0, 0.0)),
                        Plane::new(aabb.position + glm::vec4(0.0, 0.0, aabb.height * 2.0, 0.0), glm::vec4(0.0, 0.0, 1.0, 0.0)),
                        Plane::new(aabb.position + glm::vec4(0.0, 0.0, 0.0, 0.0), glm::vec4(0.0, 0.0, -1.0, 0.0)),
                    ];

                    //Check if the line segment hit any of the AABB planes
                    for plane in &planes {
                        if let Some(_) = segment_hit_plane(plane, &segment) {
                            let dist = point_plane_distance(&glm::vec3_to_vec4(&camera_position), plane);
                            camera_position += (camera_hit_sphere_radius - dist) * glm::vec4_to_vec3(&plane.normal);
                            break;
                        }
                    }
                }
            }
        }

        //The user is considered to be always standing on the ground in tracking space
        player.tracked_segment = xrutil::tracked_player_segment(&view_space, &tracking_space, last_xr_render_time, &world_from_tracking);

        //Check side collision with AABBs
        //We reduce this to a circle vs rectangle in the xy plane
        for opt_aabb in collision_aabbs.iter() {
            if let Some(aabb) = opt_aabb {
                if player.tracking_position.z + glm::epsilon::<f32>() < aabb.position.z + aabb.height &&
                   player.tracked_segment.p0.z - glm::epsilon::<f32>() > aabb.position.z {
                    let closest_point_on_aabb = glm::vec3(
                        clamp(player.tracked_segment.p1.x, aabb.position.x - aabb.width, aabb.position.x + aabb.width),
                        clamp(player.tracked_segment.p1.y, aabb.position.y - aabb.depth, aabb.position.y + aabb.depth),
                        0.0
                    );
                    let focus = glm::vec3(player.tracked_segment.p1.x, player.tracked_segment.p1.y, 0.0);

                    let distance = glm::distance(&closest_point_on_aabb, &focus);
                    if distance > 0.0 && distance < player.radius {
                        let vec = glm::normalize(&(focus - closest_point_on_aabb));
                        player.tracking_position += (player.radius - distance) * vec;
                    }
                }
            }
        }

        //Terrain collision with various objects
        let mut down_facing_tris = Vec::new();
        let mut walkable_tris = Vec::new();
        let mut too_steep_tris = Vec::new();

        //We try to do all work related to terrain collision here in order
        //to avoid iterating over all of the triangles more than once
        for i in (0..terrain.indices.len()).step_by(3) {
            let triangle = get_terrain_triangle(&terrain, i);                              //Get the triangle in question
            let triangle_plane = Plane::new(
                glm::vec4(triangle.a.x, triangle.a.y, triangle.a.z, 1.0),
                glm::vec4(triangle.normal.x, triangle.normal.y, triangle.normal.z, 0.0)
            );

            //Check if this triangle is hitting the camera
            {
                let seg = LineSegment {
                    p0: glm::vec4(last_camera_position.x, last_camera_position.y, last_camera_position.z, 1.0),
                    p1: glm::vec4(camera_position.x, camera_position.y, camera_position.z, 1.0)
                };
                let dist1 = point_plane_distance(&seg.p1, &triangle_plane);

                let collision_point = { 
                    let p = camera_position + triangle.normal * -dist1 ;
                    glm::vec4(p.x, p.y, p.z, 1.0)
                };
                
                if robust_point_in_triangle(&glm::vec4_to_vec3(&collision_point), &triangle) && f32::abs(dist1) < camera_hit_sphere_radius {
                    camera_position += triangle.normal * (camera_hit_sphere_radius - dist1);
                }
            }

            //For this triangle, determine if the player is in the triangle in the xy plane
            if simple_point_in_triangle(&glm::vec2(player.tracked_segment.p1.x, player.tracked_segment.p1.y), &glm::vec2(triangle.a.x, triangle.a.y), &glm::vec2(triangle.b.x, triangle.b.y), &glm::vec2(triangle.c.x, triangle.c.y)) {
                let dot = glm::dot(&triangle.normal, &glm::vec3(0.0, 0.0, 1.0));

                //Sort into one of the arrays based on the normal's angle with +z
                if dot < 0.0 { down_facing_tris.push(triangle_plane); }
                else if dot >= MIN_NORMAL_LIKENESS { walkable_tris.push(triangle_plane); }
                else { too_steep_tris.push(triangle_plane); }
            }
        }

        //Check if we're bonking any of the down-facing triangles
        for tri in down_facing_tris.iter() {
            //If the distance to the triangle is within the range (-distance, distance), eject the player
            let distance = point_plane_distance(&player.tracked_segment.p0, &tri);
            if f32::abs(distance) < player.radius {
                player.tracking_position += glm::vec4_to_vec3(&tri.normal) * (player.radius - distance);
            }
        }

        //User collision section
        const MIN_NORMAL_LIKENESS: f32 = 0.7;
        match player.movement_state {
            MoveState::Falling => {
                let mut done_checking = false;
                let standing_segment = LineSegment {
                    p0: player.last_tracked_segment.p1 + glm::vec4(0.0, 0.0, 0.05, 0.0),
                    p1: player.tracked_segment.p1
                };
                let swept_segment = LineSegment {
                    p0: 0.5 * player.tracked_segment.p0 + 0.5 * player.tracked_segment.p1,
                    p1: 0.5 * player.last_tracked_segment.p0 + 0.5 * player.last_tracked_segment.p1
                };

                //Check for AABB top collision
                if !done_checking {
                    for opt_aabb in collision_aabbs.iter() {
                        if let Some(aabb) = opt_aabb {
                            let (plane, aabb_boundaries) = aabb_get_top_plane(&aabb);
                            if let Some(point) = segment_hit_bounded_plane(&plane, &standing_segment, &aabb_boundaries) {
                                resolve_collision_floor(&mut player, &point);
                                done_checking = true;
                            }
                        }
                    }
                }

                //Check for AABB bottom collision
                if !done_checking {
                    let head_segment = LineSegment {
                        p0: player.last_tracked_segment.p0,
                        p1: player.tracked_segment.p0 + glm::vec4(0.0, 0.0, player.radius, 0.0)
                    };

                    for opt_aabb in collision_aabbs.iter() {
                        if let Some(aabb) = opt_aabb {
                            let (plane, aabb_boundaries) = aabb_get_bottom_plane(&aabb);
                            if let Some(point) = segment_hit_bounded_plane(&plane, &head_segment, &aabb_boundaries) {
                                let len = glm::length(&(point - head_segment.p1));
                                player.tracking_velocity.z = 0.0;
                                player.tracking_position += glm::vec3(0.0, 0.0, -len - player.radius);
                                
                                done_checking = true;
                            }
                        }
                    }
                }

                //Check if we're standing on the terrain mesh
                //Check walkable tris
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&standing_segment, &walkable_tris) {
                        resolve_collision_floor(&mut player, &collision_plane.point);
                        done_checking = true;
                    }
                }

                //Check too steep tris
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&standing_segment, &too_steep_tris) {
                        resolve_collision_too_steep(&mut player, &collision_plane);
                        done_checking = true;
                    }
                }

                //Check walkable tris with the swept segment
                if !done_checking {                    
                    if let Some(collision_plane) = segment_plane_tallest_collision(&swept_segment, &walkable_tris) {
                        resolve_collision_floor(&mut player, &collision_plane.point);
                        done_checking = true;
                    }
                }

                //Check too steep tris
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&swept_segment, &too_steep_tris) {
                        resolve_collision_too_steep(&mut player, &collision_plane);
                        done_checking = true;
                    }
                }

                //Local collision resolution function
                fn resolve_collision_floor(player: &mut Player, collision_point: &glm::TVec4<f32>,) {
                    player.tracking_velocity = glm::zero();
                    player.tracking_position += glm::vec4_to_vec3(&(collision_point - player.tracked_segment.p1));
                    player.jumps_remaining = Player::MAX_JUMPS;
                    player.movement_state = MoveState::Grounded;
                }

                fn resolve_collision_too_steep(player: &mut Player, collision_plane: &Plane) {                    
                    let distance_to_steep_tri = -point_plane_distance(&player.tracked_segment.p1, collision_plane);
                    player.tracking_velocity.x = 0.0;
                    player.tracking_velocity.y = 0.0;
                    player.tracking_position += glm::vec4_to_vec3(&(collision_plane.normal * distance_to_steep_tri));
                    player.movement_state = MoveState::Sliding;
                }
            }
            MoveState::Sliding => {
                let mut done_checking = false;

                //Check the walkable tris
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&player.tracked_segment, &walkable_tris) {
                        player.tracking_velocity = glm::zero();
                        player.tracking_position += glm::vec4_to_vec3(&(collision_plane.point - player.tracked_segment.p1));
                        player.jumps_remaining = Player::MAX_JUMPS;
                        player.movement_state = MoveState::Grounded;
                        done_checking = true;                        
                    }
                }

                //Check the too steep tris
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&player.tracked_segment, &too_steep_tris) {
                        let distance_to_steep_tri = -point_plane_distance(&player.tracked_segment.p1, &collision_plane);
                        player.tracking_position += glm::vec4_to_vec3(&(collision_plane.normal * distance_to_steep_tri));
                        done_checking = true;
                    }
                }

                //Set player falling if not colliding with any tris
                if !done_checking {
                    set_player_falling(&mut player);
                }
            }
            MoveState::Grounded => {
                let mut done_checking = false;
                let standing_segment = LineSegment {
                    p0: player.tracked_segment.p1 + glm::vec4(0.0, 0.0, 0.5, 1.0),
                    p1: player.tracked_segment.p1 - glm::vec4(0.0, 0.0, 0.2, 1.0)
                };
                let swept_segment = LineSegment {
                    p0: 0.5 * player.tracked_segment.p0 + 0.5 * player.tracked_segment.p1,
                    p1: 0.5 * player.last_tracked_segment.p0 + 0.5 * player.last_tracked_segment.p1
                };

                //Check the AABBs
                if !done_checking {
                    for opt_aabb in collision_aabbs.iter() {
                        if let Some(aabb) = opt_aabb {
                            let (plane, aabb_boundaries) = aabb_get_top_plane(&aabb);
                            if let Some(point) = segment_hit_bounded_plane(&plane, &standing_segment, &aabb_boundaries) {
                                resolve_collision_walkable(&mut player, &point, &mut done_checking);
                                break;
                            }
                        }
                    }
                }

                //Check if we're standing on a walkable triangle
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&standing_segment, &walkable_tris) {
                        resolve_collision_walkable(&mut player, &collision_plane.point, &mut done_checking);
                    }
                }                
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&standing_segment, &too_steep_tris) {
                        resolve_collision_too_steep(&mut player, &collision_plane.point);
                    }
                }
                
                //Check if we're standing on a too-steep triangle
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&swept_segment, &walkable_tris) {
                        resolve_collision_walkable(&mut player, &collision_plane.point, &mut done_checking);
                    }
                }                
                if !done_checking {
                    if let Some(collision_plane) = segment_plane_tallest_collision(&swept_segment, &too_steep_tris) {
                        resolve_collision_too_steep(&mut player, &collision_plane.point);
                    }
                }

                //If we're still not grounded, we're falling
                if !done_checking {
                    set_player_falling(&mut player);
                }

                //Local collision resolution functions

                fn resolve_collision_walkable(player: &mut Player, collision_point: &glm::TVec4<f32>, done_checking: &mut bool) {
                    eject_player(player, collision_point);
                    player.movement_state = MoveState::Grounded;
                    *done_checking = true;
                }
                fn resolve_collision_too_steep(player: &mut Player, collision_point: &glm::TVec4<f32>) {
                    eject_player(player, collision_point);
                    player.movement_state = MoveState::Sliding;
                }
                fn eject_player(player: &mut Player, collision_point: &glm::TVec4<f32>) {
                    player.tracking_velocity = glm::zero();
                    player.tracking_position += glm::vec4_to_vec3(&(collision_point - player.tracked_segment.p1));
                }
            }
        }

        //After all collision processing has been completed, update the tracking space matrices once more
        world_from_tracking = glm::translation(&player.tracking_position);
        tracking_from_world = glm::affine_inverse(world_from_tracking);

        shadow_view = glm::look_at(&scene_data.sun_direction, &glm::zero(), &glm::vec3(0.0, 0.0, 1.0));
        scene_data.shadow_matrix = shadow_projection * shadow_view;

        player.last_tracked_segment = player.tracked_segment.clone();
        last_camera_position = camera_position;

        //Pre-render phase

        //Draw ImGui
        if do_imgui {
            let win = imgui::Window::new(im_str!("Hacking window"));
            if let Some(win_token) = win.begin(&imgui_ui) {
                if let Some(_) = &xr_instance {
                    imgui_ui.same_line(0.0);
                    if imgui_ui.button(im_str!("Reset player position"), [0.0, 32.0]) {
                        reset_player_position(&mut player);
                    }
                }
                imgui_ui.text(im_str!("FPS: {:.2}\tFrame: {}", framerate, frame_count));
                imgui_ui.checkbox(im_str!("Wireframe view"), &mut wireframe);
                imgui_ui.checkbox(im_str!("TRUE wireframe view"), &mut true_wireframe);
                imgui_ui.checkbox(im_str!("Complex normals"), &mut scene_data.complex_normals);
                if let Some(_) = &xr_instance {
                    imgui_ui.checkbox(im_str!("HMD Point-of-view"), &mut hmd_pov);
                }
                imgui_ui.separator();

                //Do visualization radio selection
                imgui_ui.text(im_str!("Debug visualization options:"));
                if imgui_ui.radio_button_bool(im_str!("Visualize normals"), scene_data.fragment_flag == FragmentFlag::Normals) { handle_radio_flag(&mut scene_data.fragment_flag, FragmentFlag::Normals, FragmentFlag::Default); }
                if imgui_ui.radio_button_bool(im_str!("Visualize LOD zones"), scene_data.fragment_flag == FragmentFlag::LodZones) { handle_radio_flag(&mut scene_data.fragment_flag, FragmentFlag::LodZones, FragmentFlag::Default); }
                if imgui_ui.radio_button_bool(im_str!("Visualize how shadowed"), scene_data.fragment_flag == FragmentFlag::Shadowed) { handle_radio_flag(&mut scene_data.fragment_flag, FragmentFlag::Shadowed, FragmentFlag::Default); }
                imgui_ui.separator();

                imgui_ui.text(im_str!("Lighting controls:"));
                let ambient_slider = Slider::new(im_str!("Ambient strength")).range(RangeInclusive::new(0.0, 0.5));
                ambient_slider.build(&imgui_ui, &mut scene_data.ambient_strength);

                let sun_color_editor = ColorEdit::new(im_str!("Sun color"), EditableColor::Float3(&mut scene_data.sun_color));
                if sun_color_editor.build(&imgui_ui) {}

                imgui_ui.separator();

                //Fullscreen button
                if imgui_ui.button(im_str!("Toggle fullscreen"), [0.0, 32.0]) {
                    //Toggle window fullscreen
                    if !is_fullscreen {
                        is_fullscreen = true;
                        glfw.with_primary_monitor_mut(|_, opt_monitor| {
                            if let Some(monitor) = opt_monitor {
                                let pos = monitor.get_pos();
                                if let Some(mode) = monitor.get_video_mode() {
                                    resize_main_window(&mut window, &mut default_framebuffer, &mut screen_state, glm::vec2(mode.width, mode.height), pos, WindowMode::FullScreen(monitor));
                                }
                            }
                        });
                    } else {
                        is_fullscreen = false;
                        let window_size = get_window_size(&config);
                        resize_main_window(&mut window, &mut default_framebuffer, &mut screen_state, window_size, (200, 200), WindowMode::Windowed);
                    }
                }
                imgui_ui.same_line(0.0);

                //Do quit button
                if imgui_ui.button(im_str!("Quit"), [0.0, 32.0]) { window.set_should_close(true); }

                //End the window
                win_token.end(&imgui_ui);
            }
        }

        //Create a view matrix from the camera state
        {
            let new_view_matrix = glm::rotation(camera_orientation.y, &glm::vec3(1.0, 0.0, 0.0)) *
                                  glm::rotation(camera_orientation.x, &glm::vec3(0.0, 0.0, 1.0)) *
                                  glm::translation(&(-camera_position));
            screen_state.update_view(new_view_matrix);
        }

        //Render
        unsafe {
            //Enable depth test and backface culling for 3D rendering
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
            gl::Disable(gl::SCISSOR_TEST);      //Disabling scissor test because it gets enabled before 2D rendering

            if wireframe { gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE); }

            //Render into HMD
            match (&xr_session, &mut xr_swapchains, &xr_swapchain_size, &xr_swapchain_rendertarget, &xr_swapchain_images, &mut xr_framewaiter, &mut xr_framestream, &tracking_space) {
                (Some(session), Some(swapchains), Some(sc_size), Some(sc_rendertarget), Some(sc_images), Some(framewaiter), Some(framestream), Some(t_space)) => {
                    let swapchain_size = glm::vec2(sc_size.x as GLint, sc_size.y as GLint);
                    match framewaiter.wait() {
                        Ok(wait_info) => {
                            last_xr_render_time = wait_info.predicted_display_time;
                            framestream.begin().unwrap();
                            let (viewflags, views) = session.locate_views(xr::ViewConfigurationType::PRIMARY_STEREO, wait_info.predicted_display_time, t_space).unwrap();
                            
                            //Fetch the hand poses from the runtime
                            let left_hand_pose = xrutil::locate_space(&left_hand_grip_space, &tracking_space, wait_info.predicted_display_time);
                            let right_hand_pose = xrutil::locate_space(&right_hand_grip_space, &tracking_space, wait_info.predicted_display_time);
                            let right_hand_aim_pose = xrutil::locate_space(&right_hand_aim_space, &tracking_space, wait_info.predicted_display_time);

                            //Right here is where we want to update the controller object's model matrix
                            xrutil::entity_pose_update(&mut scene_data, left_wand_entity_index, left_hand_pose, &world_from_tracking);
                            xrutil::entity_pose_update(&mut scene_data, right_wand_entity_index, right_hand_pose, &world_from_tracking);

                            if let Some(p) = right_hand_aim_pose {
                                if let Some(entity) = scene_data.get_single_entity(water_cylinder_entity_index) {
                                    entity.model_matrix = xrutil::pose_to_mat4(&p, &world_from_tracking) * glm::scaling(&water_pillar_scale);
                                }
                            }

                            //Render shadow map
                            shadow_rendertarget.bind();
                            render_shadows(&scene_data);

                            for i in 0..views.len() {
                                let image_index = swapchains[i].acquire_image().unwrap();
                                swapchains[i].wait_image(xr::Duration::INFINITE).unwrap();

                                //Compute view projection matrix
                                //We have to translate to right-handed z-up from right-handed y-up
                                let eye_pose = views[i].pose;
                                let fov = views[i].fov;
                                let view_matrix = xrutil::pose_to_viewmat(&eye_pose, &tracking_from_world);
                                let eye_world_matrix = xrutil::pose_to_mat4(&eye_pose, &world_from_tracking);

                                //Use the fov to get the t, b, l, and r values of the perspective matrix
                                let near_value = NEAR_DISTANCE;
                                let far_value = FAR_DISTANCE;
                                let l = near_value * f32::tan(fov.angle_left);
                                let r = near_value * f32::tan(fov.angle_right);
                                let t = near_value * f32::tan(fov.angle_up);
                                let b = near_value * f32::tan(fov.angle_down);
                                let perspective = glm::mat4(
                                    2.0 * near_value / (r - l), 0.0, (r + l) / (r - l), 0.0,
                                    0.0, 2.0 * near_value / (t - b), (t + b) / (t - b), 0.0,
                                    0.0, 0.0, -(far_value + near_value) / (far_value - near_value), -2.0 * far_value * near_value / (far_value - near_value),
                                    0.0, 0.0, -1.0, 0.0
                                );

                                //Actually rendering
                                sc_rendertarget.bind();   //Rendering into an MSAA rendertarget
                                let view_data = ViewData::new(
                                    glm::vec3(eye_world_matrix[12], eye_world_matrix[13], eye_world_matrix[14]),
                                    view_matrix,
                                    perspective
                                );
                                render_main_scene(&scene_data, &view_data);

                                //Blit the MSAA image into the swapchain image
                                let color_texture = sc_images[i][image_index as usize];
                                gl::BindFramebuffer(gl::FRAMEBUFFER, xr_swapchain_framebuffer);
                                gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, color_texture, 0);
                                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, sc_rendertarget.framebuffer.name);
                                gl::BlitFramebuffer(0, 0, sc_size.x as GLint, sc_size.y as GLint, 0, 0, sc_size.x as GLint, sc_size.y as GLint, gl::COLOR_BUFFER_BIT, gl::NEAREST);

                                swapchains[i].release_image().unwrap();
                            }

                            //End the frame
                            //TODO: Figure out why image_array_index has to always be zero now
                            let end_result = framestream.end(wait_info.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE,
                                &[&xr::CompositionLayerProjection::new()
                                    .space(t_space)
                                    .views(&[
                                        xr::CompositionLayerProjectionView::new()
                                            .pose(views[0].pose)
                                            .fov(views[0].fov)
                                            .sub_image( 
                                                xr::SwapchainSubImage::new()
                                                    .swapchain(&swapchains[0])
                                                    .image_array_index(0)
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
                                                    .image_array_index(0)
                                                    .image_rect(xr::Rect2Di {
                                                        offset: xr::Offset2Di { x: 0, y: 0 },
                                                        extent: xr::Extent2Di {width: swapchain_size.x, height: swapchain_size.y}
                                                    })
                                            )
                                    ])
                                ]
                            );

                            if let Err(e) = end_result {
                                println!("Framestream end error: {}", e);
                            }

                            //Draw the companion view if we're showing HMD POV
                            if hmd_pov {
                                if let Some(pose) = xrutil::locate_space(&view_space, &tracking_space, wait_info.predicted_display_time) {
                                    let v_mat = xrutil::pose_to_viewmat(&pose, &tracking_from_world);
                                    let v_world_pos = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                                    let view_state = ViewData::new(
                                        glm::vec3(v_world_pos[12], v_world_pos[13], v_world_pos[14]),
                                        v_mat,
                                        *screen_state.get_clipping_from_view()
                                    );
                                    default_framebuffer.bind();
                                    render_main_scene(&scene_data, &view_state);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Error doing framewaiter.wait(): {}", e);
                        }
                    }
                }
                _ => {}
            }

            //Main window rendering
            if !hmd_pov {
                //Render shadows
                shadow_rendertarget.bind();
                render_shadows(&scene_data);

                //Render main scene
                let freecam_viewdata = ViewData::new(
                    camera_position,
                    *screen_state.get_view_from_world(),
                    *screen_state.get_clipping_from_view()
                );
                default_framebuffer.bind();
                render_main_scene(&scene_data, &freecam_viewdata);
            }

            //Render 2D elements
            gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);  //Make sure we're not doing wireframe rendering
            gl::Disable(gl::DEPTH_TEST);                    //Disable depth testing
            gl::Disable(gl::CULL_FACE);                     //Disable any face culling
            gl::Enable(gl::SCISSOR_TEST);
            if true_wireframe { gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE); }

            //Render Dear ImGui
            gl::UseProgram(imgui_program);
            glutil::bind_matrix4(imgui_program, "projection", screen_state.get_clipping_from_screen());
            {
                let draw_data = imgui_ui.render();
                
                if draw_data.total_vtx_count > 0 {                    
                    for list in draw_data.draw_lists() {
                        let vert_size = 8;
                        let mut verts = vec![0.0; list.vtx_buffer().len() * vert_size];
                        let mut inds = vec![0; list.idx_buffer().len()];

                        let mut current_index = 0;
                        let idx_buffer = list.idx_buffer();
                        for idx in idx_buffer.iter() {
                            inds[current_index] = *idx;
                            current_index += 1;
                        }

                        let mut current_vertex = 0;
                        let vtx_buffer = list.vtx_buffer();
                        for vtx in vtx_buffer.iter() {
                            verts[current_vertex * vert_size] = vtx.pos[0];
                            verts[current_vertex * vert_size + 1] = vtx.pos[1];
                            verts[current_vertex * vert_size + 2] = vtx.uv[0];
                            verts[current_vertex * vert_size + 3] = vtx.uv[1];
    
                            verts[current_vertex * vert_size + 4] = vtx.col[0] as f32 / 255.0;
                            verts[current_vertex * vert_size + 5] = vtx.col[1] as f32 / 255.0;
                            verts[current_vertex * vert_size + 6] = vtx.col[2] as f32 / 255.0;
                            verts[current_vertex * vert_size + 7] = vtx.col[3] as f32 / 255.0;
    
                            current_vertex += 1;
                        }

                        let imgui_vao = glutil::create_vertex_array_object(&verts, &inds, &[2, 2, 4]);

                        for command in list.commands() {
                            match command {
                                DrawCmd::Elements {count, cmd_params} => {
                                    gl::BindVertexArray(imgui_vao);
                                    gl::ActiveTexture(gl::TEXTURE0);
                                    gl::BindTexture(gl::TEXTURE_2D, cmd_params.texture_id.id() as GLuint);
                                    gl::Scissor(cmd_params.clip_rect[0] as GLint,
                                                screen_state.get_window_size().y as GLint - cmd_params.clip_rect[3] as GLint,
                                                (cmd_params.clip_rect[2] - cmd_params.clip_rect[0]) as GLint,
                                                (cmd_params.clip_rect[3] - cmd_params.clip_rect[1]) as GLint
                                    );
                                    gl::DrawElementsBaseVertex(gl::TRIANGLES, count as GLint, gl::UNSIGNED_SHORT, (cmd_params.idx_offset * size_of::<GLushort>()) as _, cmd_params.vtx_offset as GLint);
                                }
                                DrawCmd::ResetRenderState => {
                                    println!("DrawCmd::ResetRenderState.");
                                }
                                DrawCmd::RawCallback {..} => {
                                    println!("DrawCmd::RawCallback.");
                                }
                            }
                        }
                        
                        gl::DeleteVertexArrays(1, &imgui_vao);
                    }
                }
            }
        }

        window.swap_buffers();
    }
}
