//#![windows_subsystem = "windows"]
#![allow(non_snake_case)]
extern crate nalgebra_glm as glm;
extern crate openxr as xr;
extern crate tinyfiledialogs as tfd;

extern crate ozy_engine as ozy;

mod audio;
mod gamestate;
mod network;
mod structs;
mod render;
mod routines;
mod traits;
mod xrutil;

use glfw::{Action, Context, Key, SwapInterval, Window, WindowEvent, WindowHint, WindowMode};
use gl::types::*;
use imgui::{ColorEdit, DrawCmd, EditableColor, FontAtlasRefMut, MenuItem, Slider, TextureId};
use noise::NoiseFn;
use std::collections::HashMap;
use std::fs::{File, read_dir};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::process::exit;
use std::mem::size_of;
use std::net::SocketAddrV4;
use std::os::raw::c_void;
use std::str::FromStr;
use std::sync::mpsc;
use std::time::{Instant};
use strum::EnumCount;
use tfd::MessageBoxIcon;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use ozy::{glutil, io};
use ozy::render::{Framebuffer, RenderTarget, TextureKeeper};
use ozy::routines::uniform_scale;
use ozy::structs::OptionVec;
use ozy::collision::*;

use crate::audio::{AudioCommand, SoundEffectRequest};
use crate::gamestate::*;
use crate::structs::*;
use crate::routines::*;
use crate::render::{PointLight, MAX_POINT_LIGHTS, NEAR_DISTANCE, FAR_DISTANCE, STANDARD_TRANSFORM_ATTRIBUTE, STANDARD_HIGHLIGHTED_ATTRIBUTE, DEBUG_TRANSFORM_ATTRIBUTE, DEBUG_COLOR_ATTRIBUTE, DEBUG_HIGHLIGHTED_ATTRIBUTE};
use crate::render::{CascadedShadowMap, FragmentFlag, PostEffectFlag, RenderEntity, SceneData, ViewData};
use crate::traits::SphereCollider;
use crate::network::NetworkCommand;

#[cfg(windows)]
use winapi::{um::{winuser::GetWindowDC, wingdi::wglGetCurrentContext}};

const EPSILON: f32 = 0.00001;
const VELOCITY_CAP: f32 = 50.0;        //m/s
const ACCELERATION_GRAVITY: f32 = 20.0;        //20.0 m/s^2

//Default texture parameters for a 2D image texture
const DEFAULT_TEX_PARAMS: [(GLenum, GLenum); 4] = [  
    (gl::TEXTURE_WRAP_S, gl::REPEAT),
    (gl::TEXTURE_WRAP_T, gl::REPEAT),
    (gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR),
    (gl::TEXTURE_MAG_FILTER, gl::LINEAR)
];

fn get_lookup_texture_pixels(count: usize) -> usize {
    f32::ceil(count as f32 / 8.0) as usize
}

fn queue_debug_sphere(sphere_queue: &mut Vec<DebugSphere>, position: glm::TVec3<f32>, color: glm::TVec4<f32>, radius: f32, highlighted: bool) {
    let s = DebugSphere {
        position,
        color,
        radius,
        highlighted
    };
    sphere_queue.push(s);
}

fn LUT_pixels_from_flags(flags: &[bool]) -> Vec<u8> {
    let triangle_count = flags.len();
    let width = get_lookup_texture_pixels(triangle_count);
    let mut pixels = vec![0u8; width];            
    for i in 0..triangle_count {
        let p_idx = i / 8;
        let bit = i % 8;
        let flag = flags[i] as u8;
        pixels[p_idx] |= flag << bit;
    }
    pixels
}

fn main() {    
    let Z_UP = glm::vec3(0.0, 0.0, 1.0);

    //Initialize the configuration data
    let config = {
        //If we can't read from the config file, we create one with the default values
        match Configuration::from_file(Configuration::CONFIG_FILEPATH) {
            Some(cfg) => { cfg }
            None => {
                let mut int_options = HashMap::new();
                let mut float_options = HashMap::new();
                let mut string_options = HashMap::new();
                int_options.insert(String::from(Configuration::WINDOWED_WIDTH), 1280);
                int_options.insert(String::from(Configuration::WINDOWED_HEIGHT), 720);
                float_options.insert(String::from(Configuration::BGM_VOLUME), 10.0);
                string_options.insert(String::from(Configuration::LEVEL_NAME), String::from("toon_level"));
                string_options.insert(String::from(Configuration::MUSIC_NAME), String::from(audio::DEFAULT_BGM_PATH));
                let c = Configuration {
                    int_options,
                    float_options,
                    string_options
                };
                c.to_file(Configuration::CONFIG_FILEPATH);
                c
            }
        }
    };


    //Do a bunch of OpenXR initialization
    //Initialize the OpenXR instance
    let xr_instance = {
        let openxr_entry = xr::Entry::linked();
        let app_info = xr::ApplicationInfo {
            application_name: "xr_prototype",
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
                tfd::message_box_ok("XR initialization error", "OpenXR implementation does not support OpenGL!", MessageBoxIcon::Error);
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

    //Get the xr system id
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
        Some(views) => {
            let size = glm::vec2(views[0].recommended_image_rect_width, views[0].recommended_image_rect_height);
            println!("Eye image resolution: {}x{}", size.x, size.y);
            Some(size)
        }
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
    let xr_standard_actionset = match &xr_instance {
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
    let left_trigger_path = xrutil::make_path(&xr_instance, xrutil::LEFT_TRIGGER);
    let left_b_path = xrutil::make_path(&xr_instance, xrutil::LEFT_B_BUTTON);
    let left_y_path = xrutil::make_path(&xr_instance, xrutil::LEFT_Y_BUTTON);
    let left_stick_vector_path = xrutil::make_path(&xr_instance, xrutil::LEFT_STICK_VECTOR2);
    let left_trackpad_vector_path = xrutil::make_path(&xr_instance, xrutil::LEFT_TRACKPAD_VECTOR2);
    let left_trackpad_click_path = xrutil::make_path(&xr_instance, xrutil::LEFT_TRACKPAD_CLICK);
    let right_trigger_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRIGGER_FLOAT);
    let right_grip_pose_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_GRIP_POSE);
    let right_aim_pose_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_AIM_POSE);
    let right_trackpad_force_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRACKPAD_FORCE);
    let right_trackpad_click_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_TRACKPAD_CLICK);
    let right_a_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_A_BUTTON_BOOL);
    let right_b_path = xrutil::make_path(&xr_instance, xrutil::RIGHT_B_BUTTON);

    //Create the hand subaction paths
    let left_hand_subaction_path = xrutil::make_path(&xr_instance, xr::USER_HAND_LEFT);
    let right_hand_subaction_path = xrutil::make_path(&xr_instance, xr::USER_HAND_RIGHT);

    //Create the XrActions
    let left_hand_grip_action = xrutil::make_action(&left_hand_subaction_path, &xr_standard_actionset, "left_hand_pose", "Left hand pose");
    let left_hand_aim_action = xrutil::make_action::<xr::Posef>(&left_hand_subaction_path, &xr_standard_actionset, "left_hand_aim", "Left hand aim");
    let right_hand_grip_action = xrutil::make_action(&right_hand_subaction_path, &xr_standard_actionset, "right_hand_pose", "Right hand pose");
    let right_hand_aim_action = xrutil::make_action(&right_hand_subaction_path, &xr_standard_actionset, "right_hand_aim", "Right hand aim");
    let player_move_action = xrutil::make_action::<xr::Vector2f>(&left_hand_subaction_path, &xr_standard_actionset, "player_move", "Player movement");
    let left_gadget_action = xrutil::make_action::<f32>(&left_hand_subaction_path, &xr_standard_actionset, "left_hand_gadget", "Left hand gadget");
    let right_gadget_action = xrutil::make_action::<f32>(&right_hand_subaction_path, &xr_standard_actionset, "right_hand_gadget", "Right hand gadget");
    let left_switch_gadget = xrutil::make_action::<bool>(&left_hand_subaction_path, &xr_standard_actionset, "left_switch_gadget", "Left hand switch gadget");    
    let go_home_action = xrutil::make_action::<bool>(&right_hand_subaction_path, &xr_standard_actionset, "item_menu", "Interact with item menu");
    let right_switch_gadget = xrutil::make_action::<bool>(&right_hand_subaction_path, &xr_standard_actionset, "right_switch_gadget", "Right hand switch gadget");


    //Suggest interaction profile bindings
    if let Some(inst) = &xr_instance {
        //All VR setups will have these be the same
        let pose_bindings = [
            (&left_hand_grip_action, left_grip_pose_path),
            (&left_hand_aim_action, left_aim_pose_path),
            (&right_hand_grip_action, right_grip_pose_path),
            (&right_hand_aim_action, right_aim_pose_path),
        ];
        let float_bindings = [
            (&left_gadget_action, left_trigger_path),
            (&right_gadget_action, right_trigger_path)
        ];

        //Unique bindings
        let index_bools = [
            (&go_home_action, right_trackpad_force_path),
            (&left_switch_gadget, left_b_path),
            (&right_switch_gadget, right_b_path)
        ];
        let vive_bools = [
            (&go_home_action, None),
            (&left_switch_gadget, left_trackpad_click_path),
            (&right_switch_gadget, right_trackpad_click_path)
        ];
        let oculus_bools = [
            (&go_home_action, right_a_path),
            (&left_switch_gadget, left_y_path),
            (&right_switch_gadget, right_b_path)
        ];
        let unique_bools = [&index_bools, &vive_bools, &oculus_bools];

        let index_vec2s = [(&player_move_action, left_stick_vector_path)];
        let vive_vec2s = [(&player_move_action, left_trackpad_vector_path)];
        let oculus_vec2s = [(&player_move_action, left_stick_vector_path)];
        let unique_vec2s = [&index_vec2s, &vive_vec2s, &oculus_vec2s];

        let bindings_count = 10;
        let mut index_bindings = Vec::with_capacity(bindings_count);
        let mut vive_bindings = Vec::with_capacity(bindings_count);
        let mut oculus_bindings = Vec::with_capacity(bindings_count);
        let binding_arrays = [&mut index_bindings, &mut vive_bindings, &mut oculus_bindings];
        let interaction_profiles = [xrutil::VALVE_INDEX_INTERACTION_PROFILE, xrutil::HTC_VIVE_INTERACTION_PROFILE, xrutil::OCULUS_TOUCH_INTERACTION_PROFILE];

        //Define fn to push a potential binding to an array
        fn push_binding<'a, T: xr::ActionTy>(array: &mut Vec<xr::Binding<'a>>, binding: (&'a Option<xr::Action<T>>, Option<xr::Path>)) {            
            if let (Some(action), Some(path)) = binding {
                array.push(xr::Binding::new(action, path));
            }
        }

        //Do generic bindings
        for i in 0..binding_arrays.len() {
            for binding in pose_bindings {
                push_binding(binding_arrays[i], binding);
            }
            for binding in float_bindings {
                push_binding(binding_arrays[i], binding);
            }
        }

        //Do unique bindings
        for i in 0..unique_bools.len() {
            for binding in unique_bools[i] {
                push_binding(binding_arrays[i], *binding);
            }
        }
        for i in 0..unique_vec2s.len() {
            for binding in unique_vec2s[i] {
                push_binding(binding_arrays[i], *binding);
            }
        }

        for i in 0..interaction_profiles.len() {
            xrutil::suggest_bindings(inst, interaction_profiles[i], binding_arrays[i]);
        }
    }

    //Initializing GLFW and creating a window

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

    let mut window_size = get_window_size(&config);
    //Create the window
	glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    let (mut window, events) = match glfw.create_window(window_size.x, window_size.y, "THCATO", glfw::WindowMode::Windowed) {
        Some(stuff) => { stuff }
        None => { panic!("Unable to create a window!"); }
    };
    window.set_resizable(false);
    //window.set_decorated(false);

    //Center window on the screen
    glfw.with_primary_monitor_mut(|_, opt_monitor|{
        if let Some(monitor) = opt_monitor {
            let size = monitor.get_physical_size();
            window.set_pos(size.0 / 2, size.1 / 2);
        }
    });

    //Enable polling for various event types
    window.set_key_polling(true);
    window.set_mouse_button_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_scroll_polling(true);
    window.set_framebuffer_size_polling(true);
    window.set_char_polling(true);

    //Load OpenGL function pointers
    //This must be called before any other gl::* functions
    gl::load_with(|symbol| window.get_proc_address(symbol));

    //OpenGL static configuration
	unsafe {
        gl::DepthFunc(gl::LEQUAL);										//Pass the fragment with the smallest z-value.
		gl::Enable(gl::FRAMEBUFFER_SRGB); 								//Enable automatic linear->SRGB space conversion
        gl::Enable(gl::MULTISAMPLE);                                    //Enable MSAA
        gl::Enable(gl::BLEND);											//Enable alpha blending
		gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);			//Set blend func to (Cs * alpha + Cd * (1.0 - alpha))
        //gl::ClearColor(0.26, 0.4, 0.46, 1.0);							//Set the clear color
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);

		#[cfg(gloutput)]
		{
            use std::ptr;
			gl::Enable(gl::DEBUG_OUTPUT);									                                    //Enable verbose debug output
			gl::Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);						                                    //Synchronously call the debug callback function
			gl::DebugMessageCallback(Some(ozy::glutil::gl_debug_callback), ptr::null());		                        //Register the debug callback
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
    match (&xr_session, &xr_standard_actionset) {
        (Some(session), Some(actionset)) => {
            if let Err(e) = session.attach_action_sets(&[&actionset]) {
                println!("Unable to attach action sets: {}", e);
            }
        }
        _ => {}
    }

    //Define tracking space with z-up instead of the default y-up so that it matches world space
    let space_pose = {
        let quat = glm::quat_rotation(&Z_UP, &glm::vec3(0.0, 1.0, 0.0));
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
    let tracking_space = xrutil::make_reference_space(&xr_session, xr::ReferenceSpaceType::STAGE, space_pose);                          //Create tracking space
    let view_space = xrutil::make_reference_space(&xr_session, xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY);                      //Create view space
    
    let left_hand_grip_space = xrutil::make_actionspace(&xr_session, left_hand_subaction_path, &left_hand_grip_action, space_pose);     //Create left hand grip space
    let left_hand_aim_space = xrutil::make_actionspace(&xr_session, left_hand_subaction_path, &left_hand_aim_action, space_pose);       //Create left hand aim space
    let right_hand_grip_space = xrutil::make_actionspace(&xr_session, right_hand_subaction_path, &right_hand_grip_action, space_pose);  //Create right hand grip space
    let right_hand_aim_space = xrutil::make_actionspace(&xr_session, right_hand_subaction_path, &right_hand_aim_action, space_pose);    //Create right hand aim space

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
    //The swapchain image is made the color attachment of this fbo each frame for the purposes of blitting
    let xr_swapchain_framebuffer = unsafe {
        let mut p = 0;
        gl::GenFramebuffers(1, &mut p);
        p
    };

    //MSAA rendertarget which will have the scene rendered into it before blitting to the actual HMD swapchain image
    //This gets around the fact that SteamVR refuses to allocate MSAA rendertargets :) :) :)
    let xr_swapchain_rendertarget = match xr_swapchain_size {
        Some(size) => unsafe { Some(RenderTarget::new_multisampled((size.x as GLint, size.y as GLint), render::MSAA_SAMPLES as GLint, gl::SRGB8_ALPHA8)) }
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
    let standard_program = compile_shader_or_crash(&[(gl::VERTEX_SHADER, "shaders/standard.vert"), (gl::FRAGMENT_SHADER, "shaders/standard.frag")]);
    let debug_program = compile_shader_or_crash(&[(gl::VERTEX_SHADER, "shaders/debug.vert"), (gl::FRAGMENT_SHADER, "shaders/debug.frag")]);
    let shadow_program = compile_shader_or_crash(&[(gl::VERTEX_SHADER, "shaders/shadow.vert"), (gl::FRAGMENT_SHADER, "shaders/shadow.frag")]);
    let skybox_program = compile_shader_or_crash(&[(gl::VERTEX_SHADER, "shaders/skybox.vert"), (gl::FRAGMENT_SHADER, "shaders/skybox.frag")]);
    let imgui_program = compile_shader_or_crash(&[(gl::VERTEX_SHADER, "shaders/ui/imgui.vert"), (gl::FRAGMENT_SHADER, "shaders/ui/imgui.frag")]);
    let postfx_program = compile_shader_or_crash(&[(gl::COMPUTE_SHADER, "shaders/postfx.comp")]);
    
    //Initialize default framebuffer
    let mut default_framebuffer = Framebuffer {
        name: 0,
        size: (window_size.x as GLsizei, window_size.y as GLsizei),
        clear_flags: gl::DEPTH_BUFFER_BIT | gl::COLOR_BUFFER_BIT,
        cull_face: gl::BACK
    };

    //Creating default rendertarget
    let mut core_rt = unsafe {
        RenderTarget::new_multisampled(
            (window_size.x as GLsizei, window_size.y as GLsizei),
            render::MSAA_SAMPLES as GLint,
            gl::SRGB8_ALPHA8
        )
    };

    //Also create textures for ping-pong image effects
    let mut ping_rt = unsafe {
        RenderTarget::new((window_size.x as GLint, window_size.y as GLint), gl::SRGB8_ALPHA8)
    };
    let mut pong_rt = unsafe {
        RenderTarget::new((window_size.x as GLint, window_size.y as GLint), gl::SRGB8_ALPHA8)
    };

    //Creating Dear ImGui context
    let mut imgui_context = imgui::Context::create();
    imgui_context.style_mut().use_dark_colors();
    {
        let io = imgui_context.io_mut();
        io.display_size[0] = window_size.x as f32;
        io.display_size[1] = window_size.y as f32;
        
        //Set up keyboard index map
        io.key_map[imgui::Key::Tab as usize] = Key::Tab as u32;
        io.key_map[imgui::Key::LeftArrow as usize] = Key::Left as u32;
        io.key_map[imgui::Key::RightArrow as usize] = Key::Right as u32;
        io.key_map[imgui::Key::UpArrow as usize] = Key::Up as u32;
        io.key_map[imgui::Key::DownArrow as usize] = Key::Down as u32;
        io.key_map[imgui::Key::PageDown as usize] = Key::PageDown as u32;
        io.key_map[imgui::Key::PageUp as usize] = Key::PageUp as u32;
        io.key_map[imgui::Key::Home as usize] = Key::Home as u32;
        io.key_map[imgui::Key::End as usize] = Key::End as u32;
        io.key_map[imgui::Key::Insert as usize] = Key::Insert as u32;
        io.key_map[imgui::Key::Delete as usize] = Key::Delete as u32;
        io.key_map[imgui::Key::Backspace as usize] = Key::Backspace as u32;
        io.key_map[imgui::Key::Space as usize] = Key::Space as u32;
        io.key_map[imgui::Key::Enter as usize] = Key::Enter as u32;
        io.key_map[imgui::Key::KeyPadEnter as usize] = Key::KpEnter as u32;
        io.key_map[imgui::Key::A as usize] = Key::A as u32;
        io.key_map[imgui::Key::C as usize] = Key::C as u32;
        io.key_map[imgui::Key::V as usize] = Key::V as u32;
        io.key_map[imgui::Key::X as usize] = Key::X as u32;
        io.key_map[imgui::Key::Y as usize] = Key::Y as u32;
        io.key_map[imgui::Key::Z as usize] = Key::Z as u32;
    }

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
            glutil::apply_texture_parameters(gl::TEXTURE_2D, &font_atlas_params);
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RED as GLsizei, font_atlas.width as GLsizei, font_atlas.height as GLsizei, 0, gl::RED, gl::UNSIGNED_BYTE, font_atlas.data.as_ptr() as _);
            atlas.tex_id = TextureId::new(tex as usize);  //Giving Dear Imgui a reference to the font atlas GPU texture
        }
        FontAtlasRefMut::Shared(_) => {
            panic!("Not dealing with this case.");
        }
    };

    //User interface state
    let mut do_imgui = true;
    let mut debug_vis_menu = false;
    let mut postfx_menu = false;
    let mut env_menu = false;
    let mut entity_panel = false;
    let mut server_connection_dialogue = false;
    let mut destination_string = String::with_capacity(64);
    
    let default_camera_position = glm::vec3(0.0, -8.0, 5.5);
    let mut camera = {
        let position = default_camera_position;
        let fov_radians = glm::half_pi();

        let view_from_world = glm::identity();
		let clipping_from_view = glm::perspective_zo(window_size.x as f32 / window_size.y as f32, fov_radians, render::NEAR_DISTANCE, render::FAR_DISTANCE);
        let aspect_ratio = window_size.x as f32 / window_size.y as f32;
        let clipping_from_world = clipping_from_view * view_from_world;
        let world_from_clipping = glm::affine_inverse(clipping_from_world);
		let world_from_view = glm::affine_inverse(view_from_world);
        let clipping_from_screen = clip_from_screen(window_size);

        Camera {
            position,
            last_position: position,
            view_space_velocity: glm::zero(),
            orientation: glm::vec2(0.0, -glm::half_pi::<f32>() * 0.6),
            is_colliding: true,
            using_mouselook: false,
            radius: 0.5,
            speed: 5.0,
            aspect_ratio,
            fov_radians,
            view_from_world,
            clipping_from_view,
            clipping_from_world,
            world_from_clipping,
            world_from_view,
            clipping_from_screen
        }
    };

    //Mouse state
    let mut mouse = Mouse {
        clicked: false,
        was_clicked: false,
        screen_space_pos: glm::zero()
    };

    //Initialize scene data struct
    let mut scene_data = SceneData::default();
    scene_data.skybox_program = skybox_program;

    //Initialize shadow data
    let cascade_size = 2048;
    let shadow_rendertarget = unsafe { RenderTarget::new_shadow((cascade_size * render::SHADOW_CASCADE_COUNT as GLint, cascade_size)) };
    scene_data.sun_shadow_map = CascadedShadowMap::new(shadow_rendertarget, shadow_program, cascade_size);
    scene_data.depth_program = shadow_program;

    //Computing cascade distances
    {
        //Manually picking the cascade distances because math is hard
        //The shadow cascade distances are negative bc they apply to view space
        let view_distances = &mut scene_data.sun_shadow_map.view_space_distances;
        view_distances[0] = -(render::NEAR_DISTANCE);
        view_distances[1] = -(render::NEAR_DISTANCE + 5.0);
        view_distances[2] = -(render::NEAR_DISTANCE + 15.0);
        view_distances[3] = -(render::NEAR_DISTANCE + 25.0);
        view_distances[4] = -(render::NEAR_DISTANCE + 75.0);
        view_distances[5] = -(render::NEAR_DISTANCE + 125.0);

        //Compute the clip space distances and save them in the scene_data struct
        for i in 0..view_distances.len() {
            let p = camera.clipping_from_view * glm::vec4(0.0, 0.0, view_distances[i], 1.0);
            scene_data.sun_shadow_map.clip_space_distances[i] = p.z;
        }
    }

    scene_data.point_lights_ubo = unsafe {
        //Gen buffer
        let mut ubo = 0;
        gl::GenBuffers(1, &mut ubo);
        gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

        //Upload the buffer
        let floats_per_light = 9;
        let buffer = vec![0.0f32; MAX_POINT_LIGHTS * floats_per_light];
        gl::BufferData(
            gl::UNIFORM_BUFFER,
            (buffer.len() * size_of::<GLfloat>()) as GLsizeiptr,
            &buffer[0] as *const GLfloat as *const c_void,
            gl::DYNAMIC_DRAW
        );

        //Bind the point light ubo
        gl::UseProgram(standard_program);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, render::POINT_LIGHTS_BINDING_POINT, ubo);

        ubo
    };


    //Initialize texture caching struct
    let mut texture_keeper = TextureKeeper::new();

    //Load Totoro graphics
    let totoro_re_index = unsafe {
        let mut re = RenderEntity::from_ozy(
            "models/totoro.ozy",
            standard_program,
            64,
            STANDARD_TRANSFORM_ATTRIBUTE,
            &mut texture_keeper,
            &DEFAULT_TEX_PARAMS
        );
        re.init_new_instanced_buffer(1, STANDARD_HIGHLIGHTED_ATTRIBUTE, RenderEntity::HIGHLIGHTED_BUFFER_INDEX);

        scene_data.opaque_entities.insert(re)
    };
    
    //Matrices for relating tracking space and world space
    let mut world_from_tracking = glm::identity();
    let mut tracking_from_world;

    let mut left_sticky_grabbing = false;
    let mut right_sticky_grabbing = false;

    let mut world_state = {
        let level_name = match config.string_options.get(Configuration::LEVEL_NAME) {
            Some(name) => { name }
            None => { "testmap" }
        };

        let terrain = Terrain::from_ozt(&format!("models/{}.ozt", level_name));
        println!("Loaded {} collision triangles from {}.ozt", terrain.face_normals.len(), level_name);
        let collision = StaticCollision::new(terrain);
        let mut word = WorldState {
            player: Player::new(glm::zero(), glm::zero()),
            totoros: OptionVec::with_capacity(64),
            selected_totoro: None,
            collision,
            opaque_terrain_indices: Vec::new(),
            transparent_terrain_indices: Vec::new(),
            skybox_strings: Vec::new(),
            level_name: String::new(),
            active_skybox_index: 0,
            delta_timescale: 1.0
        };

        //Load the scene data from the level file
        load_lvl(level_name, &mut word, &mut scene_data, &mut texture_keeper, standard_program);
        load_ent(&format!("maps/{}.ent", level_name), &mut scene_data, &mut word);

        word
    };

    //Make RenderEntity for visualizing collision tris
    let terrain_re_index = unsafe {
        let inds = &world_state.collision.terrain.indices;
        let mut verts = vec![0.0; world_state.collision.terrain.vertices.len() * 6];
        for i in 0..world_state.collision.terrain.vertices.len() {
            let v = &world_state.collision.terrain.vertices[i];
            verts[6 * i] = v.x;
            verts[6 * i + 1] = v.y;
            verts[6 * i + 2] = v.z;
            verts[6 * i + 3] = 0.0;
            verts[6 * i + 4] = 0.0;
            verts[6 * i + 5] = 0.0;
        }

        let vao = glutil::create_vertex_array_object(&verts, inds, &[3, 3]);
        let mut re = RenderEntity::from_vao(vao, debug_program, inds.len(), 1, DEBUG_TRANSFORM_ATTRIBUTE, false);
        re.ignore_depth = true;
        re.init_new_instanced_buffer(4, DEBUG_COLOR_ATTRIBUTE, RenderEntity::COLOR_BUFFER_INDEX);

        let color = [1.0, 0.0, 1.0, 0.2];
        re.update_color_buffer(&color, DEBUG_COLOR_ATTRIBUTE);

        //Create lookup texture for selected triangles
        {
            gl::GenTextures(1, &mut re.lookup_texture);
            gl::BindTexture(gl::TEXTURE_1D, re.lookup_texture);

            let simple_tex_params = [
                (gl::TEXTURE_WRAP_S, gl::REPEAT),
                (gl::TEXTURE_WRAP_T, gl::REPEAT),
                (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
                (gl::TEXTURE_MAG_FILTER, gl::NEAREST)
            ];
            glutil::apply_texture_parameters(gl::TEXTURE_1D, &simple_tex_params);
            
            let pixels = LUT_pixels_from_flags(&world_state.collision.grabbable_flags);
            gl::TexImage1D(gl::TEXTURE_1D, 0, gl::R8UI as GLint, pixels.len() as GLsizei, 0, gl::RED_INTEGER, gl::UNSIGNED_BYTE, &pixels[0] as *const u8 as *const c_void);
        }

        scene_data.transparent_entities.insert(re)
    };

    //Create debug sphere render entity
    let debug_sphere_re_index = unsafe {
        let segments = 32;
        let rings = 32;
        let vao = ozy::prims::debug_sphere_vao(1.0, segments, rings);

        let mut re = RenderEntity::from_vao(
            vao,
            debug_program,
            ozy::prims::sphere_index_count(segments, rings),
            64,
            DEBUG_TRANSFORM_ATTRIBUTE,
            false
        );
        re.init_new_instanced_buffer(4, DEBUG_COLOR_ATTRIBUTE, RenderEntity::COLOR_BUFFER_INDEX);
        re.init_new_instanced_buffer(1, DEBUG_HIGHLIGHTED_ATTRIBUTE, RenderEntity::HIGHLIGHTED_BUFFER_INDEX);
        
        scene_data.transparent_entities.insert(re)
    };

    //Load gadget models
    let mut gadget_model_map = {
        let wand_entity = RenderEntity::from_ozy("models/wand.ozy", standard_program, 2, STANDARD_TRANSFORM_ATTRIBUTE, &mut texture_keeper, &DEFAULT_TEX_PARAMS);
        let stick_entity = RenderEntity::from_ozy("models/stick.ozy", standard_program, 2, STANDARD_TRANSFORM_ATTRIBUTE, &mut texture_keeper, &DEFAULT_TEX_PARAMS);
        let wand_index = scene_data.opaque_entities.insert(wand_entity);
        let stick_index = scene_data.opaque_entities.insert(stick_entity);

        let mut h = HashMap::with_capacity(2);
        h.insert(GadgetType::Net, wand_index);
        h.insert(GadgetType::StickyHand, stick_index);
        h.insert(GadgetType::WaterCannon, stick_index);
        h
    };

    //Gadget state setup
    let mut left_hand_gadget = GadgetType::Net;
    let mut right_hand_gadget = GadgetType::WaterCannon;

    //Water gun state
    const MAX_WATER_PRESSURE: f32 = 30.0;
    let mut water_gun_force: glm::TVec3<f32> = glm::zero();
    let mut infinite_ammo = false;
    let mut remaining_water = Gadget::MAX_ENERGY;

    //Water gun graphics data
    let mut left_water_pillar_scale: glm::TVec3<f32> = glm::zero();
    let mut right_water_pillar_scale: glm::TVec3<f32> = glm::zero();
    let water_cylinder_path = "models/water_cylinder.ozy";
    let water_cylinder_entity_index = scene_data.opaque_entities.insert(
        RenderEntity::from_ozy(
            water_cylinder_path,
            standard_program,
            2,
            STANDARD_TRANSFORM_ATTRIBUTE,
            &mut texture_keeper,
            &DEFAULT_TEX_PARAMS
        )
    );

    //Set up global flags lol
    let mut is_fullscreen = false;
    let mut wireframe = false;
    let mut true_wireframe = false;
    let mut click_action = ClickAction::Select;
    let mut last_toggled_tri = None;
    let mut hmd_pov = false;
    let mut do_vsync = true;
    let mut using_postfx = false;
    let mut screenshot_this_frame = false;
    let mut full_screenshot_this_frame = false;
    let mut turbo_clicking = false;
    let mut viewing_collision_spheres = false;
    let mut viewing_triangles = false;
    let mut viewing_player_spawn = false;
    let mut viewing_player_spheres = false;
    let mut showing_shadow_atlas = false;
    let mut viewing_point_lights = false;
    if let Some(_) = &xr_instance {
        hmd_pov = true;
        do_vsync = false;
        glfw.set_swap_interval(SwapInterval::None);
    }

    //Frame timing variables
    let mut frame_count = 0;
    let mut last_frame_instant = Instant::now();
    let mut last_xr_render_time = xr::Time::from_nanos(0);

    //Init audio system
    let mut bgm_volume = match config.float_options.get(Configuration::BGM_VOLUME) {
        Some(v) => { *v }
        None => { 10.0 }
    };
    let (audio_sender, audio_receiver) = mpsc::channel();
    audio::audio_main(audio_receiver, &config);          //This spawns a thread to run the audio system
    let mut next_named_sfx = 0;

    //Load totoro sound effects
    let yell_path = "sfx/totoro/yells";
    let totoro_yell_paths = match read_dir(yell_path) {
        Ok(iter) => {
            let mut paths = Vec::new();
            for entry in iter {
                match entry {
                    Ok(ent) => {
                        let name = format!("{}/{}", yell_path, ent.file_name().into_string().unwrap());
                        paths.push(name.clone());
                        send_or_error(&audio_sender, AudioCommand::LoadSFX(name));
                    }
                    Err(e) => {
                        tfd::message_box_ok("Audio error", &format!("Error reading sfx entry: {}", e), MessageBoxIcon::Error);
                    }
                }
            }
            paths
        }
        Err(e) => {
            tfd::message_box_ok("Audio error", &format!("Error reading sfx directory: {}", e), MessageBoxIcon::Error);
            Vec::new()
        }
    };
    let totoro_drowning_path = "sfx/totoro/drown.mp3";
    send_or_error(&audio_sender, AudioCommand::LoadSFX(String::from(totoro_drowning_path)));

    let key_directions = {
        let mut hm = HashMap::new();
        hm.insert(Key::W, glm::vec3(0.0, 0.0, -1.0));
        hm.insert(Key::S, glm::vec3(0.0, 0.0, 1.0));
        hm.insert(Key::A, glm::vec3(-1.0, 0.0, 0.0));
        hm.insert(Key::D, glm::vec3(1.0, 0.0, 0.0));
        hm.insert(Key::Q, glm::vec3(0.0, -1.0, 0.0));
        hm.insert(Key::E, glm::vec3(0.0, 1.0, 0.0));
        hm
    };

    //Init simplex noise function
    let simplex = noise::OpenSimplex::default();

    //Immediate mode interface for drawing debug spheres
    let mut debug_sphere_queue = Vec::with_capacity(64);

    //Start the network thread
    let (network_sender, network_receiver) = mpsc::channel();
    network::network_main(network_receiver);

    while !window.should_close() {
        let imgui_io = imgui_context.io_mut();
        //Compute the number of seconds since the start of the last frame (i.e at 60fps, delta_time ~= 0.016667)
        //The largest this value can be is 1.0 / 30.0
        let delta_time = {
            const MAX_DELTA_TIME: f32 = 1.0 / 30.0;
			let frame_instant = Instant::now();
			let dur = frame_instant.duration_since(last_frame_instant);
			last_frame_instant = frame_instant;
            imgui_io.delta_time = dur.as_secs_f32();

            //Don't allow game objects to have an update delta of more than a thirtieth of a second
            if imgui_io.delta_time > MAX_DELTA_TIME { MAX_DELTA_TIME }
            else { imgui_io.delta_time * world_state.delta_timescale }
        };
        scene_data.elapsed_time += delta_time;
        frame_count += 1;
        let framerate = imgui_io.framerate;

        //Sync OpenXR actions
        if let (Some(session), Some(controller_actionset)) = (&xr_session, &xr_standard_actionset) {
            if let Err(e) = session.sync_actions(&[xr::ActiveActionSet::new(controller_actionset)]) {
                println!("Unable to sync actions: {}", e);
            }
        }

        //Get action states
        let move_stick_state = xrutil::get_actionstate(&xr_session, &player_move_action);
        let left_trigger_state = xrutil::get_actionstate(&xr_session, &left_gadget_action);
        let left_switch_state = xrutil::get_actionstate(&xr_session, &left_switch_gadget);
        let right_switch_state = xrutil::get_actionstate(&xr_session, &right_switch_gadget);
        let right_trigger_state = xrutil::get_actionstate(&xr_session, &right_gadget_action);
        let right_trackpad_force_state = xrutil::get_actionstate(&xr_session, &go_home_action);

        //Handle window events
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Close => { window.set_should_close(true); }
                WindowEvent::Char(c) => { imgui_io.add_input_character(c); }
                WindowEvent::Key(key, _, Action::Press, _) => {
                    imgui_io.keys_down[key as usize] = true;
                    match key_directions.get(&key) {
                        Some(dir) => {
                            camera.view_space_velocity += dir;
                        }
                        None => {
                            match key {
                                Key::Escape => { do_imgui = !do_imgui; }
                                Key::LeftShift => {
                                    camera.speed *= 5.0;
                                }
                                Key::LeftControl => {
                                    camera.speed /= 5.0;
                                }
                                Key::F2 => {
                                    full_screenshot_this_frame = true;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                WindowEvent::Key(key, _, Action::Release, _) => {
                    imgui_io.keys_down[key as usize] = false;
                    match key_directions.get(&key) {
                        Some(dir) => {
                            camera.view_space_velocity -= dir;
                        }
                        None => {
                            match key {
                                Key::LeftShift => {
                                    camera.speed /= 5.0;
                                }
                                Key::LeftControl => {
                                    camera.speed *= 5.0;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                WindowEvent::MouseButton(glfw::MouseButtonLeft, action, ..) => {
                    match action {
                        Action::Press => {
                            imgui_io.mouse_down[0] = true;
                        }
                        Action::Release => {
                            imgui_io.mouse_down[0] = false;
                            last_toggled_tri = None;
                        }
                        Action::Repeat => {}
                    }
                    mouse.clicked = imgui_io.mouse_down[0];
                }
                WindowEvent::MouseButton(glfw::MouseButtonRight, glfw::Action::Press, ..) => {
                    imgui_io.mouse_down[1] = true;
                }
                WindowEvent::MouseButton(glfw::MouseButtonRight, glfw::Action::Release, ..) => {
                    imgui_io.mouse_down[1] = false;
                    if camera.using_mouselook {
                        window.set_cursor_mode(glfw::CursorMode::Normal);
                    } else {
                        window.set_cursor_mode(glfw::CursorMode::Hidden);
                    }
                    camera.using_mouselook = !camera.using_mouselook;
                }
                WindowEvent::CursorPos(x, y) => {
                    imgui_io.mouse_pos = [x as f32, y as f32];
                    mouse.screen_space_pos = glm::vec2(x as f32, y as f32);
                    if camera.using_mouselook {
                        const CAMERA_SENSITIVITY_DAMPENING: f32 = 0.002;
                        let offset = glm::vec2(mouse.screen_space_pos.x as f32 - window_size.x as f32 / 2.0, mouse.screen_space_pos.y as f32 - window_size.y as f32 / 2.0);
                        camera.orientation += offset * CAMERA_SENSITIVITY_DAMPENING;
                        if camera.orientation.y < -glm::pi::<f32>() {
                            camera.orientation.y = -glm::pi::<f32>();
                        } else if camera.orientation.y > 0.0 {
                            camera.orientation.y = 0.0;
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
        let imgui_wants_mouse = imgui_io.want_capture_mouse;    //Save whether or not Dear Imgui is using the mouse input as of last frame
        drop(imgui_io);
        
        //Begin drawing imgui frame
        let imgui_ui = imgui_context.frame();

        //Handle player inputs
        let mut sticky_action = None;
        {
            const MOVEMENT_SPEED: f32 = 5.0;
            const DEADZONE_MAGNITUDE: f32 = 0.1;
            let player = &mut world_state.player;

            //Responding to the player's input movement vector
            if let Some(state) = &move_stick_state {
                if state.changed_since_last_sync {                            
                    if let Some(pose) = xrutil::locate_space(&left_hand_aim_space, &tracking_space, state.last_change_time) {
                        let hand_space_vec = glm::vec4(state.current_state.x, state.current_state.y, 0.0, 0.0);
                        let magnitude = glm::length(&hand_space_vec);
                        if magnitude < DEADZONE_MAGNITUDE {
                            if player.movement_state == MoveState::Grounded {                                
                                player.tracking_velocity.x = 0.0;
                                player.tracking_velocity.y = 0.0;
                            }
                        } else {
                            //World space untreated vector
                            let untreated = xrutil::pose_to_mat4(&pose, &world_from_tracking) * hand_space_vec;
                            let ugh = glm::normalize(&glm::vec3(untreated.x, untreated.y, 0.0)) * MOVEMENT_SPEED * magnitude;
                            player.tracking_velocity = glm::vec3(ugh.x, ugh.y, player.tracking_velocity.z);
                            player.movement_state = MoveState::Falling;
                        }
                    }
                }
            }

            //Gadget switching
            {
                let gadgets = [&mut left_hand_gadget, &mut right_hand_gadget];
                let states = [left_switch_state, right_switch_state];
                for i in 0..states.len() {
                    if let Some(state) = states[i] {
                        //If switch button was pressed for this hand
                        if state.changed_since_last_sync {
                            if state.current_state {
                                match gadget_model_map.get(gadgets[i]) {
                                    Some(entity_idx) => {
                                        if let Some(entity) = scene_data.opaque_entities.get_mut_element(*entity_idx) {
                                            unsafe { entity.update_single_transform(i, &glm::zero()); }
                                        }
                                    }
                                    None => { println!("No model for gadget {:?}", gadgets[i]); }
                                }

                                let new = (*gadgets[i] as usize + 1) % GadgetType::COUNT;
                                *gadgets[i] = GadgetType::from_usize(new);
                            }
                        }
                    }
                }
            }

            //Handle gadget input
            {
                let trigger_states = [left_trigger_state, right_trigger_state];
                let aim_spaces = [&left_hand_aim_space, &right_hand_aim_space];
                let gadgets = [&left_hand_gadget, &right_hand_gadget];
                let pillar_scales = [&mut left_water_pillar_scale, &mut right_water_pillar_scale];

                for i in 0..trigger_states.len() {
                    if let Some(state) = trigger_states[i] {
                        match gadgets[i] {
                            GadgetType::Net => {
                                if state.changed_since_last_sync && state.current_state == 1.0 {
                                    if player.jumps_remaining > 0 {
                                        player.tracking_velocity.z = 10.0;
                                        player.jumps_remaining -= 1;
                                    }
                                }
                            }
                            GadgetType::StickyHand => {
                                if let Some(pose) = xrutil::locate_space(aim_spaces[i], &tracking_space, last_xr_render_time) {
                                    if state.current_state > 0.5 {
                                        let hand_transform = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                                        let grip_position = glm::vec4_to_vec3(&(hand_transform * glm::vec4(0.0, 0.0, 0.0, 1.0)));

                                        if i == 0 && !left_sticky_grabbing {
                                            match player.stick_data {
                                                Some(StickData::Left(_)) => {}
                                                _ => {
                                                    sticky_action = Some(StickData::Left(grip_position));
                                                }
                                            }
                                        } else if i == 1 && !right_sticky_grabbing {
                                            match player.stick_data {
                                                Some(StickData::Right(_)) => {}
                                                _ => {
                                                    sticky_action = Some(StickData::Right(grip_position));
                                                }
                                            }
                                        }
                                    } else {
                                        let unstick = |player: &mut Player| {                                            
                                            player.stick_data = None;
                                            player.tracking_velocity = (player.tracked_segment.p0 - player.last_tracked_segment.p0) / delta_time * 2.0;
                                        };

                                        if i == 0 { left_sticky_grabbing = false; }
                                        else if i == 1 { right_sticky_grabbing = false; }

                                        match &player.stick_data {
                                            Some(StickData::Left(_)) => {
                                                if i == 0 { unstick(player); }
                                            }
                                            Some(StickData::Right(_)) => {
                                                if i == 1 { unstick(player); }
                                            }
                                            None => {}
                                        }
                                    }
                                }
                            }
                            GadgetType::WaterCannon => {
                                //Calculate the force of shooting the water gun for the left hand
                                if let Some(pose) = xrutil::locate_space(aim_spaces[i], &tracking_space, last_xr_render_time) {
                                    let hand_transform = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                                    let hand_space_vec = glm::vec4(0.0, 1.0, 0.0, 0.0);
                                    let world_space_vec = hand_transform * hand_space_vec;
                    
                                    //Calculate water gun force vector
                                    water_gun_force = glm::vec4_to_vec3(&(-state.current_state * world_space_vec));
                    
                                    if state.current_state > 0.0 {
                                        pillar_scales[i].y = 0.75;
                                        if player.movement_state != MoveState::Falling {
                                            set_player_falling(player);
                                        }
                                    }
                                }
        
                                //Apply watergun force to player
                                if !floats_equal(glm::length(&water_gun_force), 0.0) && remaining_water > 0.0 {
                                    let drain_speed = 2.0;
                                    let update_force = water_gun_force * delta_time * MAX_WATER_PRESSURE;
                                    if !infinite_ammo {
                                        remaining_water -= glm::length(&update_force) * drain_speed;
                                    }
                                    let xz_scale = remaining_water / Gadget::MAX_ENERGY;
                                    pillar_scales[i].x = xz_scale;
                                    pillar_scales[i].z = xz_scale;
                                    player.tracking_velocity += update_force;
        
                                    if let Some(entity) = scene_data.opaque_entities.get_mut_element(water_cylinder_entity_index) {
                                        //Update the water gun's pillar of water
                                        entity.uv_offset += glm::vec2(0.0, 5.0) * delta_time;
                                        entity.uv_scale.y = pillar_scales[i].y;
                                    }
                                } else {
                                    *pillar_scales[i] = glm::zero();
                                }
                            }
                        }
                    }
                }
            }

            if player.movement_state != MoveState::Falling {
                remaining_water = Gadget::MAX_ENERGY;
            }

            //Emergency respawn button
            if let Some(state) = right_trackpad_force_state {
                if state.changed_since_last_sync && state.current_state {
                    reset_player_position(player);
                }
            }

            if player.tracking_position.z < -100.0 {
                reset_player_position(player);
            }
        }

        //Match the player's stuck hand to the stick position
        //Apply gravity otherwise
        match &world_state.player.stick_data {
            Some(data) => {
                let move_to_grip = |stick_point: &glm::TVec3<f32>, aim_space: &Option<xr::Space>| {
                    let mut res = glm::zero();
                    if let Some(pose) = xrutil::locate_space(aim_space, &tracking_space, last_xr_render_time) {
                        let hand_transform = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                        let grip_position = glm::vec4_to_vec3(&(hand_transform * glm::vec4(0.0, 0.0, 0.0, 1.0)));
                        res = stick_point - grip_position;
                    }
                    res
                };

                match data {
                    StickData::Left(stick_point) => {
                        world_state.player.tracking_position += move_to_grip(&stick_point, &left_hand_aim_space);
                    }                    
                    StickData::Right(stick_point) => {
                        world_state.player.tracking_position += move_to_grip(&stick_point, &right_hand_aim_space);
                    }
                }
            }
            None => {
                //Apply gravity to the player's velocity
                if world_state.player.movement_state != MoveState::Grounded {
                    world_state.player.tracking_velocity.z -= ACCELERATION_GRAVITY * delta_time;
                }
            }
        }

        //Apply speed limit to player
        {
            if world_state.player.tracking_velocity.x > VELOCITY_CAP {
                world_state.player.tracking_velocity.x = VELOCITY_CAP;
            }
            if world_state.player.tracking_velocity.x < -VELOCITY_CAP {
                world_state.player.tracking_velocity.x = -VELOCITY_CAP;
            }
            if world_state.player.tracking_velocity.y > VELOCITY_CAP {
                world_state.player.tracking_velocity.y = VELOCITY_CAP;
            }
            if world_state.player.tracking_velocity.y < -VELOCITY_CAP {
                world_state.player.tracking_velocity.y = -VELOCITY_CAP;
            }
            if world_state.player.tracking_velocity.z > VELOCITY_CAP {
                world_state.player.tracking_velocity.z = VELOCITY_CAP;
            }
            if world_state.player.tracking_velocity.z < -VELOCITY_CAP {
                world_state.player.tracking_velocity.z = -VELOCITY_CAP;
            }
        }

        //Create capsule collider(s) for water guns
        let water_gun_colliders = {
            let trigger_states = [&left_trigger_state, &right_trigger_state];
            let gadgets = [&left_hand_gadget, &right_hand_gadget];
            let pillar_scales = [&mut left_water_pillar_scale, &mut right_water_pillar_scale];
            let aim_spaces = [&left_hand_aim_space, &right_hand_aim_space];
            let mut colliders = [None, None];

            for i in 0..2 {
                if let Some(state) = trigger_states[i] {
                    if state.current_state > 0.0 {
                        if let GadgetType::WaterCannon = gadgets[i] {
                            let mut capsule_segment = LineSegment {
                                p0: glm::zero(),
                                p1: glm::vec3(0.0, pillar_scales[i].y * 2.0, 0.0)
                            };

                            if let Some(hand_aim_pose) = xrutil::locate_space(aim_spaces[i], &tracking_space, last_xr_render_time) {
                                let transform = xrutil::pose_to_mat4(&hand_aim_pose, &world_from_tracking);
                                capsule_segment.p0 = glm::vec4_to_vec3(&(transform * glm::vec4(capsule_segment.p0.x, capsule_segment.p0.y, capsule_segment.p0.z, 1.0)));
                                capsule_segment.p1 = glm::vec4_to_vec3(&(transform * glm::vec4(capsule_segment.p1.x, capsule_segment.p1.y, capsule_segment.p1.z, 1.0)));
                            }

                            let radius = pillar_scales[i].x * 0.2;
                            if viewing_collision_spheres {
                                queue_debug_sphere(&mut debug_sphere_queue, capsule_segment.p0, glm::vec4(0.0, 0.3, 1.0, 0.5), radius, false);
                                queue_debug_sphere(&mut debug_sphere_queue, capsule_segment.p1, glm::vec4(0.0, 0.4, 1.0, 0.5), radius, false);
                            }

                            colliders[i] = Some(
                                Capsule {
                                    segment: capsule_segment,
                                    radius
                                }
                            );
                        }
                    }
                }
            }

            colliders
        };

        //Totoro update
        let totoro_base_speed = 2.0;
        let totoro_awareness_radius = 5.0;
        for i in 0..world_state.totoros.len() {
            if let Some(totoro) = world_state.totoros.get_mut_element(i) {
                let ai_time = scene_data.elapsed_time - totoro.state_timer;     //Time since last state change
                let player_is_near = glm::distance(&world_state.player.tracked_segment.p1, &totoro.position) < totoro_awareness_radius;

                //Check if the player is hitting this one
                let being_hit_by_water = {
                    let mut res = false;
                    let tot_sphere = totoro.sphere();

                    //Both hands
                    for i in 0..2 {
                        if let Some(water_gun_capsule) = &water_gun_colliders[i] {
                            let segment = &water_gun_capsule.segment;
                            let t_vector = tot_sphere.focus - segment.p0;
                            let l_vector = segment.p1 - segment.p0;
                            let length = glm::length(&l_vector);
                            let t = f32::clamp(glm::dot(&t_vector, &(l_vector / length)) / length, 0.0, 1.0);

                            let test_sphere = Sphere {
                                focus: segment.p0 + t * l_vector,
                                radius: water_gun_capsule.radius
                            };
                            if viewing_collision_spheres {
                                queue_debug_sphere(&mut debug_sphere_queue, test_sphere.focus, glm::vec4(1.0, 0.0, 0.0, 0.6), test_sphere.radius, false);
                            }

                            if spheres_collide(&test_sphere, &tot_sphere) {
                                res = true;
                                break;
                            }
                        }
                    }

                    res
                };

                //Do behavior based on AI state
                match totoro.state {
                    TotoroState::Relaxed => {
                        if player_is_near {
                            totoro.state = TotoroState::Startled;
                        } else if being_hit_by_water {
                            totoro.state = TotoroState::StartDying;
                            totoro.velocity = glm::zero();
                        } else if ai_time >= totoro.relax_duration {
                            totoro.state_timer = scene_data.elapsed_time;
                            totoro.state = TotoroState::Meandering;
                            if glm::distance(&totoro.home, &totoro.position) > EPSILON {
                                totoro.desired_forward = glm::normalize(&(totoro.home - totoro.position));
                                totoro.desired_forward.z = 0.0;
                            }
                        }
                    }
                    TotoroState::Meandering => {
                        if ai_time >= 3.0 {
                            totoro.state_timer = scene_data.elapsed_time;
                            totoro.velocity = glm::vec3(0.0, 0.0, totoro.velocity.z);
                            totoro.state = TotoroState::Relaxed;
                            totoro.relax_duration = rand::random::<f32>() * 2.0 + 1.0;
                        } else {
                            //Check if the player is nearby
                            if player_is_near {
                                totoro.state = TotoroState::Startled;
                            } else if being_hit_by_water {
                                totoro.state = TotoroState::StartDying;
                                totoro.velocity = glm::zero();
                            } else {
                                let turn_speed = totoro_base_speed * 2.0;
                                totoro.forward = glm::normalize(&lerp(&totoro.forward, &totoro.desired_forward, turn_speed * delta_time));
                                
                                if ai_time >= 1.0 {
                                    totoro.desired_forward = glm::mat4_to_mat3(&glm::rotation(0.25 * glm::quarter_pi::<f32>() * rand_binomial(), &Z_UP)) * totoro.desired_forward;
                                }

                                let v = totoro.forward * totoro_base_speed;
                                totoro.velocity = glm::vec3(v.x, v.y, totoro.velocity.z);
                            }
                        }
                    }
                    TotoroState::Startled => {
                        totoro.forward = {
                            let mut f = world_state.player.tracked_segment.p1 - totoro.position;
                            f.z = 0.0;
                            glm::normalize(&f)
                        };
                        totoro.velocity = glm::vec3(0.0, 0.0, 3.0);
                        totoro.state = TotoroState::PrePanicking;
                        totoro.state_timer = scene_data.elapsed_time;

                        if totoro_yell_paths.len() > 0 {
                            let path = totoro_yell_paths[rand::random::<usize>() % totoro_yell_paths.len()].clone();
                            let yell_req = SoundEffectRequest {
                                id: None,
                                path,
                                position: vec_to_array(totoro.position),
                                linear_gain: 200.0,
                                looping: false
                            };
                            send_or_error(&audio_sender, AudioCommand::PlaySFX(yell_req));
                        }
                    }
                    TotoroState::PrePanicking => {
                        if being_hit_by_water {
                            totoro.state = TotoroState::StartDying;
                        } else if ai_time >= 0.25 {
                            totoro.forward = {
                                let mut f = totoro.position - world_state.player.tracked_segment.p1;
                                f.z = 0.0;
                                glm::normalize(&f)
                            };
                            totoro.state = TotoroState::Panicking;
                            totoro.state_timer = scene_data.elapsed_time;
                        }
                    }
                    TotoroState::Panicking => {
                        if being_hit_by_water {
                            totoro.state = TotoroState::StartDying;
                        } else {
                            let mut new_forward = glm::normalize(&(totoro.position - world_state.player.tracked_segment.p1));
                            new_forward.z = 0.0;
                            new_forward = glm::normalize(&new_forward);
                            totoro.desired_forward = glm::vec4_to_vec3(&(glm::rotation(rand_binomial(), &Z_UP) * glm::vec3_to_vec4(&new_forward)));
                            
                            let turn_speed = totoro_base_speed * 2.0;
                            totoro.forward = lerp(&totoro.forward, &totoro.desired_forward, turn_speed * delta_time);
                            totoro.forward = glm::normalize(&totoro.forward);
                            let v = totoro.forward * totoro_base_speed;
                            totoro.velocity = glm::vec3(v.x, v.y, totoro.velocity.z);

                            if ai_time >= 4.0 {
                                totoro.state = TotoroState::Meandering;
                                totoro.state_timer = scene_data.elapsed_time;
                            }
                        }
                    }
                    TotoroState::StartDying => {
                        let drown_req = SoundEffectRequest {
                            id: Some(next_named_sfx),
                            path: String::from(totoro_drowning_path),
                            position: vec_to_array(totoro.position),
                            linear_gain: 500.0,
                            looping: true
                        };
                        totoro.drown_sfx_id = Some(next_named_sfx);
                        totoro.state = TotoroState::Dying;
                        totoro.velocity = glm::zero();
                        next_named_sfx += 1;
                        send_or_error(&audio_sender, AudioCommand::PlaySFX(drown_req));
                    }
                    TotoroState::Dying => {
                        if being_hit_by_water {
                            let base_spin_rate = glm::pi::<f32>() * 4.0;
                            let spin_rate = base_spin_rate * ((Totoro::MAX_HEALTH - totoro.health) / Totoro::MAX_HEALTH * 5.0 + 1.0);
                            let depletion_rate = Totoro::MAX_HEALTH * 4.0 / 3.0;
                            totoro.health -= depletion_rate * delta_time;
                            let ford = glm::rotation(spin_rate * delta_time, &Z_UP) * glm::vec3_to_vec4(&totoro.forward);
                            totoro.forward = glm::vec4_to_vec3(&ford);
                        } else {
                            totoro.state = TotoroState::Panicking;
                            if let Some(id) = totoro.drown_sfx_id {
                                send_or_error(&audio_sender, AudioCommand::StopSFX(id));
                                totoro.drown_sfx_id = None;
                            }
                        }
                    }
                    TotoroState::BrainDead => {}
                }

                //Apply gravity
                totoro.velocity.z -= ACCELERATION_GRAVITY * delta_time;
                if totoro.velocity.z > VELOCITY_CAP {
                    totoro.velocity.z = VELOCITY_CAP;
                }

                //Apply totoro velocity to position
                totoro.position += totoro.velocity * delta_time;

                //Queue debug sphere
                if viewing_collision_spheres {
                    let sph = totoro.sphere();
                    let highlighted = match world_state.selected_totoro {
                        Some(idx) => { idx == i }
                        None => { false }
                    };
                    queue_debug_sphere(&mut debug_sphere_queue, sph.focus, glm::vec4(0.0, 0.0, 0.5, 0.5), sph.radius, highlighted);
                }

                //Kill if below a certain point or health depleted
                if totoro.position.z < -100.0 || totoro.health <= 0.0 {
                    //Stop the drowning sfx
                    if let Some(id) = totoro.drown_sfx_id {
                        send_or_error(&audio_sender, AudioCommand::StopSFX(id));
                        totoro.drown_sfx_id = None;
                    }

                    delete_object(&mut world_state.totoros, &mut world_state.selected_totoro, i);
                }
            }
        }

        //If the user is controlling the camera, force the mouse cursor into the center of the screen
        if camera.using_mouselook {
            window.set_cursor_pos(window_size.x as f64 / 2.0, window_size.y as f64 / 2.0);
        }

        /*
        glfw.with_primary_monitor_mut(|_, opt_monitor|{
            if let Some(monitor) = opt_monitor {
                let size = monitor.get_physical_size();
                window.set_pos(size.0 / 2 + (200.0 * f32::sin(scene_data.elapsed_time)) as i32, size.1 / 2);
            }
        });
        */

        let camera_velocity = camera.speed * glm::vec4_to_vec3(&(glm::affine_inverse(camera.view_from_world) * glm::vec3_to_vec4(&camera.view_space_velocity)));
        camera.position += camera_velocity * delta_time / world_state.delta_timescale;

        //Do click action
        if !imgui_wants_mouse && mouse.clicked && (!mouse.was_clicked || turbo_clicking) {
            fn check_hit_info(hit_info: Option<(f32, usize)>, t: &mut f32, i: &mut Option<usize>, f: &mut Option<usize>, new_f: usize) {
                if let Some(info) = hit_info {
                    if info.0 < *t {
                        *t = info.0;
                        *i = Some(info.1);
                        *f = Some(new_f);
                    }
                }
            }

            let mut get_clicked_closure = |click_ray: &Ray, world_state: &mut WorldState| {
                let mut r_t = f32::INFINITY;
                let mut r_i = None;
                let mut f = None;

                let tot_hit_info = get_clicked_object(&world_state.totoros, click_ray);
                check_hit_info(tot_hit_info, &mut r_t, &mut r_i, &mut f, 0);
                let light_hit_info = get_clicked_object(&scene_data.point_lights, click_ray);
                check_hit_info(light_hit_info, &mut r_t, &mut r_i, &mut f, 1);

                world_state.selected_totoro = None;
                scene_data.selected_point_light = None;

                (f, r_i)
            };

            //Compute click ray
            let w = glm::vec2(window_size.x as f32, window_size.y as f32);
            let click_ray = compute_click_ray(&camera, w, &mouse.screen_space_pos, &camera.position);
            let terrain = &world_state.collision.terrain;

            //Branch based on which click action is active
            match click_action {
                ClickAction::CreateTotoro => {
                    //Create Totoro if the ray hit
                    if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                        let tot = Totoro::new(collision.point, scene_data.elapsed_time);
                        let i = world_state.totoros.insert(tot);
                        world_state.selected_totoro = Some(i);
                    }
                }
                ClickAction::Select => {
                    if let (Some(f), Some(idx)) =  get_clicked_closure(&click_ray, &mut world_state) {
                        match f {
                            0 => { world_state.selected_totoro = Some(idx); }
                            1 => { scene_data.selected_point_light = Some(idx); }
                            _ => {}
                        }                        
                    }
                }
                ClickAction::DeleteObject => {
                    if let (Some(f), Some(idx)) = get_clicked_closure(&click_ray, &mut world_state) {
                        match f {
                            0 => { delete_object(&mut world_state.totoros, &mut world_state.selected_totoro, idx); }
                            1 => { delete_object(&mut scene_data.point_lights, &mut scene_data.selected_point_light, idx); }
                            _ => {}
                        }
                    }
                }
                ClickAction::MoveSelectedTotoro => {
                    if let Some(idx) = world_state.selected_totoro {
                        if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                            if let Some(tot) = world_state.totoros.get_mut_element(idx) {
                                tot.position = collision.point;
                                tot.home = collision.point;
                            }
                        }
                    }
                }
                ClickAction::MovePlayerSpawn => {
                    if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                        world_state.player.spawn_position = collision.point;
                    }
                }
                ClickAction::CreatePointLight => {
                    if scene_data.point_lights.count() < render::MAX_POINT_LIGHTS { 
                        if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                            let light = PointLight::new(collision.point + glm::vec3(0.0, 0.0, 2.0), [rand::random(), rand::random(), rand::random()], 3.0);
                            let i = scene_data.point_lights.insert(light);
                            scene_data.selected_point_light = Some(i);
                        }
                    }
                }
                ClickAction::MovePointLight => {
                    if let Some(idx) = scene_data.selected_point_light {
                        if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                            if let Some(light) = scene_data.point_lights.get_mut_element(idx) {
                                light.position = collision.point + glm::vec3(0.0, 0.0, 2.0);
                            }
                        }
                    }
                }
                ClickAction::ToggleGrabbableTriangle => {
                    if let Some(collision) = ray_hit_terrain(terrain, &click_ray) {
                        let flags = &mut world_state.collision.grabbable_flags;
                        let idx = collision.triangle_index;

                        let mut do_toggle = true;
                        if let Some(last_tri_idx) = last_toggled_tri {
                            if last_tri_idx == idx {
                                do_toggle = false;
                            }
                        }

                        if do_toggle {
                            flags[idx] = !flags[idx];
                            last_toggled_tri = Some(idx);
                        }
                    } else {
                        last_toggled_tri = None;
                    }
                }
            }            
        }

        //Keep the selected triangles texture up to date
        if let Some(entity) = scene_data.transparent_entities.get_mut_element(terrain_re_index) {
            let pixels = LUT_pixels_from_flags(&world_state.collision.grabbable_flags);
            unsafe {
                gl::BindTexture(gl::TEXTURE_1D, entity.lookup_texture);
                gl::TexSubImage1D(gl::TEXTURE_1D, 0, 0, pixels.len() as GLsizei, gl::RED_INTEGER, gl::UNSIGNED_BYTE, &pixels[0] as *const u8 as *const c_void);
            }
        }
        
        //Update tracking space location
        world_state.player.tracking_position += world_state.player.tracking_velocity * delta_time;
        world_from_tracking = glm::translation(&world_state.player.tracking_position);

        if viewing_player_spawn {
            queue_debug_sphere(&mut debug_sphere_queue, world_state.player.spawn_position, glm::vec4(0.0, 0.5, 0.0, 0.5), 0.3, false);
        }

        if viewing_player_spheres {
            let segment = &world_state.player.tracked_segment;
            queue_debug_sphere(&mut debug_sphere_queue, segment.p0, glm::vec4(1.0, 0.5, 0.0, 0.5), Player::RADIUS, false);
            queue_debug_sphere(&mut debug_sphere_queue, segment.p1, glm::vec4(1.0, 0.5, 0.5, 0.5), Player::RADIUS, false);
        }

        if viewing_point_lights {
            for i in 0..scene_data.point_lights.len() {
                if let Some(light) = &scene_data.point_lights[i] {
                    let highlighted = match scene_data.selected_point_light {
                        Some(idx) => { idx == i }
                        None => { false }
                    };

                    queue_debug_sphere(
                        &mut debug_sphere_queue,
                        light.position,
                        glm::vec4(light.color[0], light.color[1], light.color[2], 0.4),
                        PointLight::COLLISION_RADIUS,
                        highlighted
                    );
                }
            }
        }

        //Collision handling section

        //The user is considered to be always standing on the ground in tracking space        
        world_state.player.last_tracked_segment = world_state.player.tracked_segment.clone();
        world_state.player.tracked_segment = xrutil::tracked_player_segment(&view_space, &tracking_space, last_xr_render_time, &world_from_tracking);

        //We try to do all work related to terrain collision here in order
        //to avoid iterating over all of the triangles more than once
        for i in (0..world_state.collision.terrain.indices.len()).step_by(3) {
            let triangle = get_terrain_triangle(&world_state.collision.terrain, i);                              //Get the triangle in question
            let triangle_plane = Plane::new(
                triangle.a,
                triangle.normal
            );

            //We create a bounding sphere for the triangle in order to do a coarse collision step
            let triangle_sphere = {
                let focus = midpoint(&triangle.c, &midpoint(&triangle.a, &triangle.b));
                let radius = glm::max3_scalar(
                    glm::distance(&focus, &triangle.a),
                    glm::distance(&focus, &triangle.b),
                    glm::distance(&focus, &triangle.c)
                );
                Sphere {
                    focus,
                    radius
                }
            };

            //Check if this triangle is hitting the camera
            if camera.is_colliding {
                let s = Sphere {
                    focus: camera.position,
                    radius: camera.radius
                };

                if let Some(vec) = triangle_collide_sphere(&s, &triangle, &triangle_sphere) {
                    camera.position += vec;
                }
            }

            //Check player capsule against triangle
            const MIN_NORMAL_LIKENESS: f32 = 0.5;
            {
                //Coarse test with sphere
                let player_sphere = Sphere {
                    focus: midpoint(&(world_state.player.tracked_segment.p0 + glm::vec3(0.0, 0.0, Player::RADIUS)), &world_state.player.tracked_segment.p1),
                    radius: glm::distance(&(world_state.player.tracked_segment.p0 + glm::vec3(0.0, 0.0, Player::RADIUS)), &world_state.player.tracked_segment.p1)
                };
                if glm::distance(&player_sphere.focus, &triangle_sphere.focus) < player_sphere.radius + triangle_sphere.radius {
                    let player_capsule = Capsule {
                        segment: LineSegment {
                            p0: world_state.player.tracked_segment.p0,
                            p1: world_state.player.tracked_segment.p1 + glm::vec3(0.0, 0.0, Player::RADIUS)
                        },
                        radius: Player::RADIUS
                    };
                    let capsule_ray = Ray {
                        origin: player_capsule.segment.p0,
                        direction: player_capsule.segment.p1 - player_capsule.segment.p0
                    };
    
                    //Finding the closest point on the triangle to the line segment of the capsule
                    let ref_point = match ray_hit_plane(&capsule_ray, &triangle_plane) {
                        Some((_, intersection)) => {
                            if robust_point_in_triangle(&intersection, &triangle) { intersection }
                            else { closest_point_on_triangle(&intersection, &triangle).1 }
                        }
                        None => { triangle.a }
                    };
                    
                    //The point on the capsule line-segment that is to be used as the focus for the sphere
                    let capsule_ref = closest_point_on_line_segment(&ref_point, &player_capsule.segment.p0, &player_capsule.segment.p1);
                    
                    //Now do a triangle-sphere test with a sphere at this reference point
                    let collision_resolution_vector = {
                        let s = Sphere {
                            focus: capsule_ref,
                            radius: Player::RADIUS
                        };
                        triangle_collide_sphere(&s, &triangle, &triangle_sphere)
                    };
                    if let Some(vec) = collision_resolution_vector {
                        if floats_equal(glm::dot(&glm::normalize(&vec), &triangle.normal), 1.0) {
                            let dot_z_up = glm::dot(&triangle.normal, &Z_UP);                        
                            if dot_z_up >= MIN_NORMAL_LIKENESS {
                                let t = (glm::dot(&triangle.normal, &(triangle.a - capsule_ref)) + Player::RADIUS) / dot_z_up;
                                world_state.player.tracking_position += Z_UP * t;
                                ground_player(&mut world_state.player, &mut remaining_water);
                            } else {
                                world_state.player.tracking_position += vec;
                            }
                        } else {
                            world_state.player.tracking_position += vec;
                        }
                    }
                }
            }

            //Resolve player's attempt to stick to a wall
            let triangle_idx = i / 3;
            if world_state.collision.grabbable_flags[triangle_idx] {
                if let Some(action) = &sticky_action {
                    fn grip_triangle(world_state: &mut WorldState, focus: glm::TVec3<f32>, radius: f32, triangle: &Triangle, triangle_sphere: &Sphere, grab_flag: &mut bool, is_left: bool) {
                        let sphere = Sphere {
                            focus,
                            radius: radius
                        };
        
                        if let Some((_, collision_point)) = triangle_sphere_collision_point(&sphere, triangle, triangle_sphere) {
                            world_state.player.tracking_position += collision_point - sphere.focus;
                            world_state.player.tracking_velocity = glm::zero();
                            *grab_flag = true;
                            
                            if is_left {
                                world_state.player.stick_data = Some(StickData::Left(collision_point));
                            } else {
                                world_state.player.stick_data = Some(StickData::Right(collision_point));
                            }
                        }
                    }

                    let stick_sphere_radius = 0.05;
                    match action {
                        StickData::Left(focus) => {
                            match world_state.player.stick_data {
                                Some(StickData::Left(_)) => {}
                                _ => {
                                    grip_triangle(&mut world_state, *focus, stick_sphere_radius, &triangle, &triangle_sphere, &mut left_sticky_grabbing, true);
                                }
                            }
                        }
                        StickData::Right(focus) => {
                            match world_state.player.stick_data {
                                Some(StickData::Right(_)) => {}
                                _ => {
                                    grip_triangle(&mut world_state, *focus, stick_sphere_radius, &triangle, &triangle_sphere, &mut right_sticky_grabbing, false);
                                }
                            }
                        }
                    }
                }
            }

            //Check totoros against triangle
            let totoros = &mut world_state.totoros;
            for i in 0..totoros.len() {
                if let Some(totoro) = totoros.get_mut_element(i) {
                    let tot_sphere = totoro.sphere();

                    if let Some(vec) = triangle_collide_sphere(&tot_sphere, &triangle, &triangle_sphere) {
                        if floats_equal(glm::dot(&glm::normalize(&vec), &triangle.normal), 1.0) {
                            let dot_z_up = glm::dot(&triangle.normal, &Z_UP);                        
                            if dot_z_up >= MIN_NORMAL_LIKENESS {
                                let t = (glm::dot(&triangle.normal, &(triangle.a - tot_sphere.focus)) + tot_sphere.radius) / dot_z_up;
                                totoro.position += Z_UP * t;
                                totoro.velocity.z = 0.0;
                            } else {
                                totoro.position += vec;
                            }
                        } else {
                            totoro.position += vec;
                        }
                    }
                }
            }
        }

        //After all collision processing has been completed, update the tracking space matrices once more
        world_from_tracking = glm::translation(&world_state.player.tracking_position);
        tracking_from_world = glm::affine_inverse(world_from_tracking);

        //Tell the audio thread about the listener's current state
        {
            //Just doing the match here to determine if the listener should be the HMD or the free camera
            let (listener_pos, listener_vel, listener_forward, listener_up) = match &xr_instance {
                Some(_) => {
                    let head_pose_mat = match xrutil::locate_space(&view_space, &tracking_space, last_xr_render_time) {
                        Some(space) => { xrutil::pose_to_mat4(&space, &world_from_tracking) }
                        None => { glm::identity() }
                    };

                    let pos = world_state.player.tracked_segment.p0;
                    let vel = pos - world_state.player.last_tracked_segment.p0;
                    let forward = glm::vec4_to_vec3(&(head_pose_mat * glm::vec4(0.0, 0.0, -1.0, 0.0)));
                    let up = glm::vec4_to_vec3(&(head_pose_mat * glm::vec4(0.0, 1.0, 0.0, 0.0)));
                    (vec_to_array(pos), vec_to_array(vel), vec_to_array(forward), vec_to_array(up))
                }
                None => {
                    let camera_vel = camera.position - camera.last_position;
                    let camera_forward = glm::vec4_to_vec3(&(camera.world_from_view * glm::vec4(0.0, 0.0, -1.0, 0.0)));
                    let camera_up = glm::vec4_to_vec3(&(camera.world_from_view * glm::vec4(0.0, 1.0, 0.0, 0.0)));
                    
                    (vec_to_array(camera.position), vec_to_array(camera_vel), vec_to_array(camera_forward), vec_to_array(camera_up))
                }
            };

            send_or_error(&audio_sender, AudioCommand::SetListenerPosition(listener_pos));
            send_or_error(&audio_sender, AudioCommand::SetListenerVelocity(listener_vel));
            send_or_error(&audio_sender, AudioCommand::SetListenerOrientation((listener_forward, listener_up)));
        }

        //Network updating section
        

        camera.last_position = camera.position;
        mouse.was_clicked = mouse.clicked;

        //Draw ImGui
        if do_imgui {
            //Sets a flag to a value or unsets the flag if it already is the value
            fn do_radio_button<F: Eq + Default>(imgui_ui: &imgui::Ui, label: &str, flag: &mut F, new_flag: F) {
                if imgui_ui.radio_button_bool(label, *flag == new_flag) { 
                    if *flag != new_flag { *flag = new_flag; }
                    else { *flag = F::default(); }
                }
            }
            fn do_button(imgui_ui: &imgui::Ui, label: &str) -> bool {
                let standard_button_size = [0.0, 32.0];
                imgui_ui.button_with_size(label, standard_button_size)
            }
            fn do_readwrite_vec3(imgui_ui: &imgui::Ui, label: &str, vector: &mut glm::TVec3<f32>) {
                let drag_speed = 0.02;
                imgui_ui.text(label);
                imgui::Drag::new("X").speed(drag_speed).build(&imgui_ui, &mut vector.x);
                imgui::Drag::new("Y").speed(drag_speed).build(&imgui_ui, &mut vector.y);
                imgui::Drag::new("Z").speed(drag_speed).build(&imgui_ui, &mut vector.z);
            }

            if let Some(win_token) = imgui::Window::new("Main menu").menu_bar(true).begin(&imgui_ui) {
                if let Some(menu_token) = imgui_ui.begin_menu_bar() {
                    if let Some(file_token) = imgui_ui.begin_menu("File") {
                        if MenuItem::new("Load level").build(&imgui_ui) {
                            if let Some(path) = tfd::open_file_dialog("Load level data", "maps/", Some((&["*.lvl"], "*.lvl"))) {
                                //Load the scene data from the level file
                                let lvl_name = Path::new(&path).file_stem().unwrap().to_str().unwrap();
                                load_lvl(lvl_name, &mut world_state, &mut scene_data, &mut texture_keeper, standard_program);
        
                                //Load terrain data
                                world_state.collision.terrain = Terrain::from_ozt(&format!("models/{}.ozt", lvl_name));
                                println!("Loaded {} collision triangles from {}.ozt", world_state.collision.terrain.face_normals.len(), world_state.level_name);
        
                                //Load entity data
                                load_ent(&format!("maps/{}.ent", lvl_name), &mut scene_data, &mut world_state);
                            }
                        }

                        if MenuItem::new("Save level").build(&imgui_ui) {
                            fn write_f32_to_buffer(bytes: &mut Vec<u8>, n: f32) {
                                let b = f32::to_le_bytes(n);
                                bytes.push(b[0]);
                                bytes.push(b[1]);
                                bytes.push(b[2]);
                                bytes.push(b[3]);
                            }
    
                            fn write_vec3_to_buffer(bytes: &mut Vec<u8>, n: glm::TVec3<f32>) {
                                write_f32_to_buffer(bytes, n.x);
                                write_f32_to_buffer(bytes, n.y);
                                write_f32_to_buffer(bytes, n.z);
                            }
    
                            fn write_u32_to_buffer(bytes: &mut Vec<u8>, n: u32) {
                                let b = u32::to_le_bytes(n);
                                bytes.push(b[0]);
                                bytes.push(b[1]);
                                bytes.push(b[2]);
                                bytes.push(b[3]);
                            }

                            fn write_u8_to_buffer(bytes: &mut Vec<u8>, n: u8) {
                                bytes.push(n);
                            }
    
                            let save_error = |e: std::io::Error| {
                                tfd::message_box_ok("Error saving level data", &format!("Could not save level data:\n{}", e), MessageBoxIcon::Error);
                            };
                            
                            match File::create(format!("maps/{}.ent", world_state.level_name)) {
                                Ok(mut file) => {
                                    let totoros = &world_state.totoros;
                                    io::write_pascal_strings(&mut file, &[&world_state.skybox_strings[world_state.active_skybox_index]]);
    
                                    let floats_to_write = [
                                        scene_data.ambient_strength,
                                        scene_data.sun_pitch,
                                        scene_data.sun_yaw,
                                        scene_data.sun_color[0],
                                        scene_data.sun_color[1],
                                        scene_data.sun_color[2],
                                        scene_data.shininess_lower_bound,
                                        scene_data.shininess_upper_bound,
                                        scene_data.sun_size,
                                        world_state.player.spawn_position.x,
                                        world_state.player.spawn_position.y,
                                        world_state.player.spawn_position.z
                                    ];
                                    let floats_per_totoro = 4;
                                    let floats_per_light = 9;
                                    
                                    //Precompute final filesize
                                    let grab_flags = &world_state.collision.grabbable_flags;
                                    let size = {
                                        let totoro_floats = floats_per_totoro * totoros.count();
                                        let point_light_floats = scene_data.point_lights.count() * floats_per_light;
                                        let grab_bool_bytes = grab_flags.len();
                                        let stored_sizes = 3;
                                        size_of::<f32>() * (floats_to_write.len() + totoro_floats + point_light_floats) + grab_bool_bytes + size_of::<u32>() * stored_sizes
                                    };
    
                                    //Convert to raw bytes and write to file
                                    let mut bytes = Vec::with_capacity(size);
                                    for i in 0..floats_to_write.len() {
                                        write_f32_to_buffer(&mut bytes, floats_to_write[i]);
                                    }

                                    //Write grabbable triangle data
                                    write_u32_to_buffer(&mut bytes, grab_flags.len() as u32);
                                    for i in 0..grab_flags.len() {
                                        let f = grab_flags[i] as u8;
                                        write_u8_to_buffer(&mut bytes, f);
                                    }
    
                                    //Write totoro data
                                    write_u32_to_buffer(&mut bytes, totoros.count() as u32);
                                    for i in 0..totoros.len() {
                                        if let Some(tot) = &totoros[i] {
                                            write_vec3_to_buffer(&mut bytes, tot.home);
                                            write_f32_to_buffer(&mut bytes, tot.scale);
                                        }
                                    }
    
                                    //Write lights data
                                    write_u32_to_buffer(&mut bytes, scene_data.point_lights.count() as u32);
                                    for i in 0..scene_data.point_lights.len() {
                                        if let Some(light) = &scene_data.point_lights[i] {
                                            write_vec3_to_buffer(&mut bytes, light.position);
                                            write_vec3_to_buffer(&mut bytes, glm::vec3(light.color[0], light.color[1], light.color[2]));
                                            write_f32_to_buffer(&mut bytes, light.power);
                                            write_f32_to_buffer(&mut bytes, light.flicker_amplitude);
                                            write_f32_to_buffer(&mut bytes, light.flicker_timescale);
                                        }
                                    }
    
                                    match file.write(&bytes) {
                                        Ok(n) => {
                                            println!("Saved {}.ent ({}/{} bytes)", world_state.level_name, n, size);
                                        }
                                        Err(e) => {
                                            save_error(e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    save_error(e);
                                }
                            }
                        }
                        
                        if MenuItem::new("Take screenshot").build(&imgui_ui) { screenshot_this_frame = true; }

                        if MenuItem::new("Exit").build(&imgui_ui) { window.set_should_close(true); }

                        file_token.end();
                    }
                    
                    if let Some(edit_token) = imgui_ui.begin_menu("Game") {
                        if MenuItem::new("Entity panel").build(&imgui_ui) {
                            entity_panel = true;
                        }

                        edit_token.end();
                    }

                    if let Some(graphics_token) = imgui_ui.begin_menu("Graphics") {
                        if MenuItem::new("Environment").build(&imgui_ui) { env_menu = true; }
                        if MenuItem::new("Debug").build(&imgui_ui) { debug_vis_menu = true; }
                        if MenuItem::new("PostFX").build(&imgui_ui) { postfx_menu = true; }

                        graphics_token.end();
                    }

                    if let Some(network_token) = imgui_ui.begin_menu("Network") {
                        if MenuItem::new("Connect to server").build(&imgui_ui) { server_connection_dialogue = true; }
                        if MenuItem::new("Disconnect").build(&imgui_ui) {  }

                        network_token.end();
                    }

                    if let Some(window_token) = imgui_ui.begin_menu("Window") {
                        if MenuItem::new("Toggle fullscreen").build(&imgui_ui) {
                            unsafe {
                                //Toggle window fullscreen
                                window.set_decorated(is_fullscreen);
                                if !is_fullscreen {
                                    glfw.with_primary_monitor_mut(|_, opt_monitor| {
                                        if let Some(monitor) = opt_monitor {
                                            let pos = monitor.get_pos();
                                            if let Some(mode) = monitor.get_video_mode() {
                                                window_size = glm::vec2(mode.width, mode.height);
                                                resize_main_window(
                                                    &mut window,
                                                    &mut core_rt,
                                                    &mut ping_rt,
                                                    &mut pong_rt,
                                                    window_size,
                                                    pos,
                                                    WindowMode::Windowed
                                                );
                                            }
                                        }
                                    });
                                } else {
                                    window_size = get_window_size(&config);
                                    resize_main_window(&mut window, &mut core_rt, &mut ping_rt, &mut pong_rt, window_size, (200, 200), WindowMode::Windowed);
                                }
                            }

                            default_framebuffer.size = core_rt.framebuffer.size;
                            is_fullscreen = !is_fullscreen;
                        }
                        
                        window_token.end();
                    }

                    menu_token.end();
                }

                imgui_ui.text(format!("Frametime: {:.2}ms\tFPS: {:.0}\tFrame: {}", delta_time * 1000.0 / world_state.delta_timescale, framerate, frame_count));
                imgui_ui.text(format!("Totoros spawned: {}", world_state.totoros.count()));
                imgui_ui.text(format!("Point lights count: {}/{}", scene_data.point_lights.count(), render::MAX_POINT_LIGHTS));
                
                if let None = &xr_instance {
                    if imgui_ui.checkbox("Lock FPS (v-sync)", &mut do_vsync) {
                        if do_vsync { glfw.set_swap_interval(SwapInterval::Sync(1)); }
                        else { glfw.set_swap_interval(SwapInterval::None); }
                    }
                }
                imgui_ui.checkbox("Camera collision", &mut camera.is_colliding);
                if let Some(_) = &xr_instance {
                    imgui_ui.checkbox("HMD Perspective", &mut hmd_pov);
                    imgui_ui.checkbox("Infinite ammo", &mut infinite_ammo);
                }
                imgui_ui.separator();
                
                //Music controls section
                imgui_ui.text("Music controls");
                if Slider::new("Master Volume", 0.0, 100.0).build(&imgui_ui, &mut bgm_volume) {
                    send_or_error(&audio_sender, AudioCommand::SetListenerGain(bgm_volume));
                }

                if do_button(&imgui_ui, "Play/Pause") {
                    send_or_error(&audio_sender, AudioCommand::PlayPause);
                }
                imgui_ui.same_line();
                if do_button(&imgui_ui, "Restart") {
                    send_or_error(&audio_sender, AudioCommand::RestartBGM);
                }
                imgui_ui.same_line();
                if do_button(&imgui_ui, "Choose mp3") {
                    send_or_error(&audio_sender, AudioCommand::SelectNewBGM);
                }

                imgui_ui.separator();

                if Slider::new("Timescale", 0.001, 2.0).build(&imgui_ui, &mut world_state.delta_timescale) {
                    send_or_error(&audio_sender, AudioCommand::SetPitchShift(world_state.delta_timescale));
                }
                
                //Reset player position button
                if let Some(_) = &xr_instance {
                    if do_button(&imgui_ui, "Reset player position") {
                        reset_player_position(&mut world_state.player);
                    }
                }

                if do_button(&imgui_ui, "Reset freecam position") {
                    camera.position = default_camera_position;
                }

                if do_button(&imgui_ui, "Help") {
                    tfd::message_box_ok("PLACEHOLDER", "Nick needs to implement a help window", MessageBoxIcon::Warning);
                }

                //End the window
                win_token.end();
            }

            //Entity panel
            if entity_panel {
                if let Some(win_token) = imgui::Window::new("Entity panel").begin(&imgui_ui) {
                    imgui_ui.checkbox("View point lights", &mut viewing_point_lights);
                    imgui_ui.checkbox("View player spawn", &mut viewing_player_spawn);
                    if imgui_ui.checkbox("View collision triangles", &mut viewing_triangles) {
                        if let Some(re) = scene_data.transparent_entities.get_mut_element(terrain_re_index) {
                            let mat = if viewing_triangles { glm::identity::<f32, 4>() }
                            else { glm::zero() };

                            re.update_transform_buffer(glm::value_ptr(&mat), DEBUG_TRANSFORM_ATTRIBUTE);
                        }
                    }
                    imgui_ui.checkbox("View collision spheres", &mut viewing_collision_spheres);
                    imgui_ui.text("Click actions");
                    do_radio_button(&imgui_ui, "Create totoro", &mut click_action, ClickAction::CreateTotoro);
                    do_radio_button(&imgui_ui, "Create light source", &mut click_action, ClickAction::CreatePointLight);
                    do_radio_button(&imgui_ui, "Delete object", &mut click_action, ClickAction::DeleteObject);
                    do_radio_button(&imgui_ui, "Move player spawn", &mut click_action, ClickAction::MovePlayerSpawn);
                    do_radio_button(&imgui_ui, "Toggle collision triangle's grabbability", &mut click_action, ClickAction::ToggleGrabbableTriangle);
                    imgui_ui.separator();
                    imgui_ui.checkbox("Turbo clicking", &mut turbo_clicking);

                    if do_button(&imgui_ui, "Delete all totoros") {
                        world_state.totoros.clear();
                        world_state.selected_totoro = None;
                    }

                    unsafe {
                        if do_button(&imgui_ui, "Clear grabbable triangles") {
                            for i in 0..world_state.collision.grabbable_flags.len() {
                                world_state.collision.grabbable_flags[i] = false;
                                if let Some(entity) = scene_data.transparent_entities.get_mut_element(terrain_re_index) {
                                    let triangle_count = world_state.collision.terrain.face_normals.len();
                                    let width = get_lookup_texture_pixels(triangle_count);
                                    let pixels = vec![0x00u8; width];
                                    gl::BindTexture(gl::TEXTURE_1D, entity.lookup_texture);
                                    gl::TexSubImage1D(gl::TEXTURE_1D, 0, 0, width as GLsizei, gl::RED_INTEGER, gl::UNSIGNED_BYTE, &pixels[0] as *const u8 as *const c_void);
                                }
                            }
                        }
                    }

                    if do_button(&imgui_ui, "Close") { entity_panel = false; }

                    win_token.end();
                }
            }

            //Window where you enter server connection info
            if server_connection_dialogue {
                if let Some(win_token) = imgui::Window::new("Connect to server").begin(&imgui_ui) {
                    imgui_ui.text(format!("Connection status: Nothing lol"));                    
                    imgui_ui.separator();

                    imgui_ui.set_next_item_width(300.0);
                    imgui::InputText::new(&imgui_ui, "Server address (<ipv4 address>:<port>)", &mut destination_string).build();

                    if do_button(&imgui_ui, "Connect") {
                        match SocketAddrV4::from_str(&destination_string) {
                            Ok(saddr) => {
                                let address = enet::Address::from(saddr);
                                send_or_error(&network_sender, NetworkCommand::AttemptConnection(address));
                            }
                            Err(e) => {
                                println!("Connect error: {}", e);
                            }
                        }
                    }
                    imgui_ui.same_line();

                    if do_button(&imgui_ui, "Disconnect") {  }

                    imgui_ui.separator();

                    if do_button(&imgui_ui, "Close") {
                        destination_string.clear();
                        server_connection_dialogue = false;
                    }

                    win_token.end();
                }
            }

            //Do graphics menu
            if debug_vis_menu {
                if let Some(win_token) = imgui::Window::new("Graphics").begin(&imgui_ui) {
                    imgui_ui.text("Graphics options");
                    imgui_ui.checkbox("Wireframe view", &mut wireframe);
                    imgui_ui.checkbox("TRUE wireframe view", &mut true_wireframe);
                    imgui_ui.checkbox("Use toon shading", &mut scene_data.toon_shading);
                    imgui_ui.checkbox("Complex normals", &mut scene_data.complex_normals);

                    imgui_ui.separator();

                    do_radio_button(&imgui_ui, "Visualize albedo", &mut scene_data.fragment_flag, FragmentFlag::Albedo);
                    do_radio_button(&imgui_ui, "Visualize normals", &mut scene_data.fragment_flag, FragmentFlag::Normals);
                    do_radio_button(&imgui_ui, "Visualize how shadowed", &mut scene_data.fragment_flag, FragmentFlag::Shadowed);
                    do_radio_button(&imgui_ui, "Visualize shadow cascades", &mut scene_data.fragment_flag, FragmentFlag::CascadeZones);

                    imgui_ui.separator();


                    imgui_ui.checkbox("View shadow atlas", &mut showing_shadow_atlas);

                    if let Some(_) = &xr_instance {
                        imgui_ui.checkbox("View player", &mut viewing_player_spheres);
                    }

                    if do_button(&imgui_ui, "Close") { debug_vis_menu = false; }

                    win_token.end();
                }
            }

            if postfx_menu {
                if let Some(win_token) = imgui::Window::new("PostFX").begin(&imgui_ui) {
                    
                    imgui_ui.checkbox("Use postfx", &mut using_postfx);
                    imgui_ui.separator();

                    do_radio_button(&imgui_ui, "Gaussian Blur", &mut scene_data.postfx_flag, PostEffectFlag::GaussianBlur);
                    do_radio_button(&imgui_ui, "Black & White", &mut scene_data.postfx_flag, PostEffectFlag::BlackWhite);
                    do_radio_button(&imgui_ui, "Glitchy", &mut scene_data.postfx_flag, PostEffectFlag::Glitchy);
                    imgui_ui.separator();

                    if do_button(&imgui_ui, "Close") { postfx_menu = false; }

                    win_token.end();
                }
            }

            //Environment options menu
            if env_menu {
                if let Some(win_token) = imgui::Window::new("Environment controls").begin(&imgui_ui) {
                    imgui_ui.text("Environment controls:");
                    Slider::new("Ambient light", 0.0, 0.5).build(&imgui_ui, &mut scene_data.ambient_strength);
                    Slider::new("Specular lower bound", 1.0, 128.0).build(&imgui_ui, &mut scene_data.shininess_lower_bound);
                    Slider::new("Specular upper bound", 1.0, 128.0).build(&imgui_ui, &mut scene_data.shininess_upper_bound);
                    Slider::new("Shadow intensity", 0.0, 1.0).build(&imgui_ui, &mut scene_data.shadow_intensity);

                    imgui_ui.separator();

                    Slider::new("Sun pitch", 0.0, glm::pi::<f32>()).build(&imgui_ui, &mut scene_data.sun_pitch);
                    Slider::new("Sun yaw", 0.0, glm::two_pi::<f32>()).build(&imgui_ui, &mut scene_data.sun_yaw);
                    Slider::new("Sun size", 0.0, 1.0).build(&imgui_ui, &mut scene_data.sun_size);
                    ColorEdit::new("Sun color", EditableColor::Float3(&mut scene_data.sun_color)).build(&imgui_ui);

                    imgui_ui.separator();

                    let mut skybox_strs = Vec::with_capacity(world_state.skybox_strings.len());
                    for i in 0..world_state.skybox_strings.len() {
                        skybox_strs.push(&world_state.skybox_strings[i]);
                    }

                    let old_skybox_index = world_state.active_skybox_index;
                    if imgui_ui.combo_simple_string("Active skybox", &mut world_state.active_skybox_index, &skybox_strs) {
                        if old_skybox_index != world_state.active_skybox_index {
                            let name = Path::new(skybox_strs[world_state.active_skybox_index]).file_name().unwrap().to_str().unwrap();
                            scene_data.skybox_cubemap = unsafe {
                                gl::DeleteTextures(1, &mut scene_data.skybox_cubemap);
                                create_skybox_cubemap(name)
                            };
                        }
                    }

                    if do_button(&imgui_ui, "Close") { env_menu = false; }

                    win_token.end();
                }
            }

            //Do selected Totoro window
            if let Some(idx) = world_state.selected_totoro {
                let tot = world_state.totoros.get_mut_element(idx).unwrap();
                if let Some(token) = imgui::Window::new(format!("Totoro #{} control panel###totoro_panel", idx)).begin(&imgui_ui) {
                    do_readwrite_vec3(&imgui_ui, "Position", &mut tot.position);
                    imgui_ui.text(format!("Velocity ({:.3}, {:.3}, {:.3})", tot.velocity.x, tot.velocity.y, tot.velocity.z));
                    imgui_ui.text(format!("AI state: {:?}", tot.state));
                    imgui_ui.text(format!("AI timer state: {:.5}/{:.5}", scene_data.elapsed_time - tot.state_timer, tot.relax_duration));
                            
                    imgui_ui.separator();
                    imgui::Slider::new("Scale", 0.1, 4.0).build(&imgui_ui, &mut tot.scale);

                    if do_button(&imgui_ui, "Toggle AI") {
                        tot.state = match tot.state {
                            TotoroState::BrainDead => { TotoroState::Relaxed }
                            _ => { TotoroState::BrainDead }
                        };
                    }
                    imgui_ui.same_line();

                    if do_button(&imgui_ui, "Kill") {
                        delete_object(&mut world_state.totoros, &mut world_state.selected_totoro, idx);
                    }

                    imgui_ui.separator();
                    do_radio_button(&imgui_ui, "Move totoro's home", &mut click_action, ClickAction::MoveSelectedTotoro);

                    if do_button(&imgui_ui, "Close") { world_state.selected_totoro = None; }

                    token.end();
                }
            }

            //Do selected point light window
            if let Some(idx) = scene_data.selected_point_light {
                let light = scene_data.point_lights.get_mut_element(idx).unwrap();
                if let Some(token) = imgui::Window::new(format!("Point light #{} control panel###point_light_panel", idx)).begin(&imgui_ui) {
                    do_readwrite_vec3(&imgui_ui, "Position", &mut light.position);
                            
                    imgui_ui.separator();
                    imgui::Slider::new("Power", 0.0, 10.0).build(&imgui_ui, &mut light.power);
                    imgui::Slider::new("Flicker amplitude", 0.0, 3.0).build(&imgui_ui, &mut light.flicker_amplitude);
                    imgui::Slider::new("Flicker timescale", 0.0, 10.0).build(&imgui_ui, &mut light.flicker_timescale);
                    
                    ColorEdit::new("Light color", EditableColor::Float3(&mut light.color)).build(&imgui_ui);

                    imgui_ui.separator();
                    do_radio_button(&imgui_ui, "Reposition light", &mut click_action, ClickAction::MovePointLight);
                    if do_button(&imgui_ui, "Delete this light") {
                        scene_data.point_lights.delete(idx);
                        scene_data.selected_point_light = None;
                    }

                    if do_button(&imgui_ui, "Close") { scene_data.selected_point_light = None; }

                    token.end();
                }
            }

            //Shadow cascade viewer
            if showing_shadow_atlas {
                let win = imgui::Window::new("Shadow atlas");
                if let Some(win_token) = win.begin(&imgui_ui) {
                    let im = imgui::Image::new(TextureId::new(scene_data.sun_shadow_map.rendertarget.texture as usize), [(cascade_size * render::SHADOW_CASCADE_COUNT as i32 / 6) as f32, (cascade_size / 6) as f32]).uv1([1.0, -1.0]);
                    im.build(&imgui_ui);

                    win_token.end();
                }
            }
        }

        //Pre-render phase

        //Update the GPU instance buffer for the Totoros
        if let Some(entity) = scene_data.opaque_entities.get_mut_element(totoro_re_index) {
            let totoros = &world_state.totoros;
            let mut highlighted_buffer = vec![0.0; totoros.count()];
            let mut transform_buffer = vec![0.0; totoros.count() * 16];
            let mut current_totoro = 0;
            for i in 0..totoros.len() {
                if let Some(totoro) = &totoros[i] {
                    //Directly constructing the rotation matrix
                    let cr = glm::cross(&Z_UP, &totoro.forward);
                    let rotation_mat = glm::mat4(
                        totoro.forward.x, cr.x, 0.0, 0.0,
                        totoro.forward.y, cr.y, 0.0, 0.0,
                        totoro.forward.z, cr.z, 1.0, 0.0,
                        0.0, 0.0, 0.0, 1.0
                    );

                    let mm = glm::translation(&totoro.position) * rotation_mat * uniform_scale(totoro.scale);
                    write_matrix_to_buffer(&mut transform_buffer, current_totoro, mm);

                    if let Some(idx) = world_state.selected_totoro {
                        if idx == i {
                            highlighted_buffer[current_totoro] = 1.0;
                        }
                    }

                    current_totoro += 1;
                }
            }

            entity.update_highlight_buffer(&highlighted_buffer, STANDARD_HIGHLIGHTED_ATTRIBUTE);
            entity.update_transform_buffer(&transform_buffer, STANDARD_TRANSFORM_ATTRIBUTE);
        }

        //Update the GPU instance buffer for the debug spheres
        if let Some(entity) = scene_data.transparent_entities.get_mut_element(debug_sphere_re_index) {
            let instances = debug_sphere_queue.len();
            let mut highlighted_buffer = vec![0.0; instances];
            let mut color_buffer = vec![0.0; instances * 4];
            let mut transform_buffer = vec![0.0; instances * 16];

            let mut idx = 0;
            for sphere in debug_sphere_queue.drain(0..instances) {
                let mm = glm::translation(&sphere.position) * uniform_scale(-sphere.radius);
                write_matrix_to_buffer(&mut transform_buffer, idx, mm);
                write_vec4_to_buffer(&mut color_buffer, idx, sphere.color);

                highlighted_buffer[idx] = if sphere.highlighted { 1.0 }
                                            else { 0.0 };
                
                idx += 1;
            }

            entity.update_highlight_buffer(&highlighted_buffer, DEBUG_HIGHLIGHTED_ATTRIBUTE);
            entity.update_transform_buffer(&transform_buffer, DEBUG_TRANSFORM_ATTRIBUTE);
            entity.update_color_buffer(&color_buffer, DEBUG_COLOR_ATTRIBUTE);
        }

        //Update the uniform buffer object of point lights
        unsafe {
            //Create the buffer
            let floats_per_light = 9; //4N+4N+Ns
        
            //Create the buffer
            let mut buffer = vec![0.0; MAX_POINT_LIGHTS * floats_per_light];        
            let mut current_light = 0;
            for i in 0..scene_data.point_lights.len() {
                if let Some(light) = &scene_data.point_lights[i] {
                    buffer[current_light * 4] = light.position.x;
                    buffer[current_light * 4 + 1] = light.position.y;
                    buffer[current_light * 4 + 2] = light.position.z;
        
                    buffer[(current_light + MAX_POINT_LIGHTS) * 4] = light.color[0];
                    buffer[(current_light + MAX_POINT_LIGHTS) * 4 + 1] = light.color[1];
                    buffer[(current_light + MAX_POINT_LIGHTS) * 4 + 2] = light.color[2];
                    
                    //Modulate power
                    let offset = light.flicker_amplitude * simplex.get([0.0, light.flicker_timescale as f64 * scene_data.elapsed_time as f64]) as f32;
                    buffer[(2 * MAX_POINT_LIGHTS) * 4 + current_light] = light.power + offset;
        
                    current_light += 1;
                }
            }
            
            gl::BindBuffer(gl::UNIFORM_BUFFER, scene_data.point_lights_ubo);
            let mut current_buffer_size = 0;
            gl::GetBufferParameteriv(gl::UNIFORM_BUFFER, gl::BUFFER_SIZE, &mut current_buffer_size);
    
            if buffer.len() * size_of::<GLfloat>() > current_buffer_size as usize {
                let mut b = 0;
                gl::DeleteBuffers(1, &scene_data.point_lights_ubo as *const u32);
                gl::GenBuffers(1, &mut b);
                scene_data.point_lights_ubo = b;
                
                gl::BindBuffer(gl::UNIFORM_BUFFER, scene_data.point_lights_ubo);
                gl::BufferData(
                    gl::UNIFORM_BUFFER,
                    (buffer.len() * size_of::<GLfloat>()) as GLsizeiptr,
                    &buffer[0] as *const GLfloat as *const c_void,
                    gl::DYNAMIC_DRAW
                );
                
                //Bind the point light ubo
                gl::UseProgram(standard_program);
                gl::BindBufferBase(gl::UNIFORM_BUFFER, render::POINT_LIGHTS_BINDING_POINT, scene_data.point_lights_ubo);
            } else if buffer.len() > 0 {
                gl::BufferSubData(
                    gl::UNIFORM_BUFFER,
                    0 as GLsizeiptr,
                    (buffer.len() * size_of::<GLfloat>()) as GLsizeiptr,
                    &buffer[0] as *const GLfloat as *const c_void
                );
            }
        }

        //Compute sun direction from pitch and yaw
        scene_data.sun_direction = glm::vec4_to_vec3(&(
            glm::rotation(scene_data.sun_yaw, &Z_UP) *
            glm::rotation(scene_data.sun_pitch, &glm::vec3(0.0, 1.0, 0.0)) *
            glm::vec4(-1.0, 0.0, 0.0, 0.0)
        ));

        //Create a view matrix from the camera state
        {
            let new_view_matrix = glm::rotation(camera.orientation.y, &glm::vec3(1.0, 0.0, 0.0)) *
                                  glm::rotation(camera.orientation.x, &Z_UP) *
                                  glm::translation(&(-camera.position));
            camera.update_view(new_view_matrix, window_size);
        }

        //Rendering
        unsafe {
            let shadow_view = glm::look_at(&(scene_data.sun_direction * 20.0), &glm::zero(), &Z_UP);

            //Setting up OpenGL state for 3D rendering
            gl::Enable(gl::DEPTH_TEST);         //Depth test
            gl::Enable(gl::CULL_FACE);          //Backface culling
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
                            let (_, views) = session.locate_views(xr::ViewConfigurationType::PRIMARY_STEREO, wait_info.predicted_display_time, t_space).unwrap();
                            
                            //Fetch the hand poses from the runtime
                            let left_grip_pose = xrutil::locate_space(&left_hand_grip_space, &tracking_space, wait_info.predicted_display_time);
                            let right_grip_pose = xrutil::locate_space(&right_hand_grip_space, &tracking_space, wait_info.predicted_display_time);
                            let left_hand_aim_pose = xrutil::locate_space(&left_hand_aim_space, &tracking_space, wait_info.predicted_display_time);
                            let right_hand_aim_pose = xrutil::locate_space(&right_hand_aim_space, &tracking_space, wait_info.predicted_display_time);

                            //Right here is where we want to update the controller objects' transforms
                            {
                                if let Some(pose) = &left_grip_pose {
                                    match gadget_model_map.get_mut(&left_hand_gadget) {
                                        Some(entity_idx) => {
                                            if let Some(entity) = scene_data.opaque_entities.get_mut_element(*entity_idx) {
                                                entity.update_single_transform(0, &xrutil::pose_to_mat4(pose, &world_from_tracking));
                                            }
                                        }
                                        None => { println!("No model for gadget {:?}", left_hand_gadget); }
                                    }
                                }

                                if let Some(pose) = &right_grip_pose {
                                    match gadget_model_map.get_mut(&right_hand_gadget) {
                                        Some(entity_idx) => {
                                            if let Some(entity) = scene_data.opaque_entities.get_mut_element(*entity_idx) {
                                                entity.update_single_transform(1, &xrutil::pose_to_mat4(pose, &world_from_tracking));
                                            }
                                        }
                                        None => { println!("No model for gadget {:?}", right_hand_gadget); }
                                    }
                                }
                            }

                            //Apply the water pillar scales
                            {
                                let poses = [left_hand_aim_pose, right_hand_aim_pose];
                                let scales = [&left_water_pillar_scale, &right_water_pillar_scale];
                                for i in 0..poses.len() {
                                    if let Some(p) = poses[i] {
                                        if let Some(entity) = scene_data.opaque_entities.get_mut_element(water_cylinder_entity_index) {
                                            let mm = xrutil::pose_to_mat4(&p, &world_from_tracking) * glm::scaling(scales[i]);
                                            entity.update_single_transform(i, &mm);
                                        }
                                    }
                                }
                            }

                            if let Some(pose) = xrutil::locate_space(&view_space, &tracking_space, wait_info.predicted_display_time) {

                                //Rendering for each eye
                                for i in 0..views.len() {
                                    let image_index = swapchains[i].acquire_image().unwrap();
                                    swapchains[i].wait_image(xr::Duration::INFINITE).unwrap();
    
                                    //Compute view projection matrix
                                    //We have to translate to right-handed z-up from right-handed y-up
                                    let eye_pose = views[i].pose;
                                    let fov = views[i].fov;
                                    let eye_view_matrix = xrutil::pose_to_viewmat(&eye_pose, &tracking_from_world);
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

                                    //Render the CSM for this eye
                                    scene_data.sun_shadow_map.matrices = render::compute_shadow_cascade_matrices(&scene_data.sun_shadow_map.view_space_distances, &shadow_view, &eye_view_matrix, &perspective);
                                    render::cascaded_shadow_map(&scene_data.sun_shadow_map, scene_data.opaque_entities.as_slice());
    
                                    //Actually rendering
                                    let view_data = ViewData::new(
                                        glm::vec3(eye_world_matrix[12], eye_world_matrix[13], eye_world_matrix[14]),
                                        eye_view_matrix,
                                        perspective
                                    );
                                    render::main_scene(&sc_rendertarget.framebuffer, &scene_data, &view_data);
    
                                    //Blit the MSAA image into the swapchain image
                                    let color_texture = sc_images[i][image_index as usize];
                                    gl::BindFramebuffer(gl::FRAMEBUFFER, xr_swapchain_framebuffer);
                                    gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, color_texture, 0);
                                    gl::BindFramebuffer(gl::READ_FRAMEBUFFER, sc_rendertarget.framebuffer.name);
                                    gl::BlitFramebuffer(0, 0, sc_size.x as GLint, sc_size.y as GLint, 0, 0, sc_size.x as GLint, sc_size.y as GLint, gl::COLOR_BUFFER_BIT, gl::NEAREST);
    
                                    swapchains[i].release_image().unwrap();
                                }

                                //Draw the companion view if we're showing HMD POV
                                let projection = camera.clipping_from_view;
                                let v_mat = xrutil::pose_to_viewmat(&pose, &tracking_from_world);
                                if hmd_pov {
                                    let v_world_pos = xrutil::pose_to_mat4(&pose, &world_from_tracking);
                                    let view_state = ViewData::new(
                                        glm::vec3(v_world_pos[12], v_world_pos[13], v_world_pos[14]),
                                        v_mat,
                                        projection
                                    );
                                    render::main_scene(&core_rt.framebuffer, &scene_data, &view_state);

                                    //Resolving the MSAA rendertarget
                                    //Both framebuffers have an internal format of gl::SRGB8_ALPHA8
                                    render::blit_full_color_buffer(&core_rt.framebuffer, &ping_rt.framebuffer);

                                    //Post-processing step
                                    if using_postfx {
                                        render::post_processing(ping_rt.color_attachment_view, window_size, postfx_program, &scene_data);
                                    }

                                    //Blit to default framebuffer
                                    render::blit_full_color_buffer(&ping_rt.framebuffer, &default_framebuffer);
                                }
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
                let projection = &camera.clipping_from_view;
                let v_mat = &camera.view_from_world;
                scene_data.sun_shadow_map.matrices = render::compute_shadow_cascade_matrices(&scene_data.sun_shadow_map.view_space_distances, &shadow_view, v_mat, projection);
                render::cascaded_shadow_map(&scene_data.sun_shadow_map, scene_data.opaque_entities.as_slice());

                //Render main scene
                let freecam_viewdata = ViewData::new(
                    camera.position,
                    camera.view_from_world,
                    camera.clipping_from_view
                );
                render::main_scene(&core_rt.framebuffer, &scene_data, &freecam_viewdata);

                //Resolving the MSAA rendertarget
                //Both framebuffers have an internal format of gl::SRGB8_ALPHA8
                render::blit_full_color_buffer(&core_rt.framebuffer, &ping_rt.framebuffer);

                //Post-processing step
                if using_postfx {
                    render::post_processing(ping_rt.color_attachment_view, window_size, postfx_program, &scene_data);
                }
                
                //Blit to default framebuffer
                render::blit_full_color_buffer(&ping_rt.framebuffer, &default_framebuffer);
            }

            //Take a screenshot here as to not get the dev gui in it
            if screenshot_this_frame {
                take_screenshot(window_size);
                screenshot_this_frame = false;
            }

            //Render 2D elements
            gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);  //Make sure we're not doing wireframe rendering
            gl::Disable(gl::DEPTH_TEST);                    //Disable depth testing
            gl::Disable(gl::CULL_FACE);                     //Disable any face culling
            gl::Enable(gl::SCISSOR_TEST);                   //Scissor test enable for Dear ImGui clipping
            gl::BindFramebuffer(gl::FRAMEBUFFER, default_framebuffer.name);
            gl::Viewport(0, 0, default_framebuffer.size.0, default_framebuffer.size.1);
            if true_wireframe { gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE); }

            //Render Dear ImGui
            gl::UseProgram(imgui_program);
            glutil::bind_matrix4(imgui_program, "projection", &camera.clipping_from_screen);
            {
                let draw_data = imgui_ui.render();
                if draw_data.total_vtx_count > 0 {
                    for list in draw_data.draw_lists() {
                        let vert_size = 8;
                        let mut verts = vec![0.0; list.vtx_buffer().len() * vert_size];

                        let mut current_vertex = 0;
                        let vtx_buffer = list.vtx_buffer();
                        for vtx in vtx_buffer.iter() {
                            let idx = current_vertex * vert_size;
                            verts[idx] = vtx.pos[0];
                            verts[idx + 1] = vtx.pos[1];
                            verts[idx + 2] = vtx.uv[0];
                            verts[idx + 3] = vtx.uv[1];    
                            verts[idx + 4] = vtx.col[0] as f32 / 255.0;
                            verts[idx + 5] = vtx.col[1] as f32 / 255.0;
                            verts[idx + 6] = vtx.col[2] as f32 / 255.0;
                            verts[idx + 7] = vtx.col[3] as f32 / 255.0;
    
                            current_vertex += 1;
                        }

                        let imgui_vao = glutil::create_vertex_array_object(&verts, list.idx_buffer(), &[2, 2, 4]);

                        for command in list.commands() {
                            match command {
                                DrawCmd::Elements {count, cmd_params} => {
                                    gl::BindVertexArray(imgui_vao.vao);
                                    gl::ActiveTexture(gl::TEXTURE0);
                                    gl::BindTexture(gl::TEXTURE_2D, cmd_params.texture_id.id() as GLuint);
                                    gl::Scissor(
                                        cmd_params.clip_rect[0] as GLint,
                                        window_size.y as GLint - cmd_params.clip_rect[3] as GLint,
                                        (cmd_params.clip_rect[2] - cmd_params.clip_rect[0]) as GLint,
                                        (cmd_params.clip_rect[3] - cmd_params.clip_rect[1]) as GLint
                                    );
                                    gl::DrawElementsBaseVertex(gl::TRIANGLES, count as GLint, gl::UNSIGNED_SHORT, (cmd_params.idx_offset * size_of::<GLushort>()) as _, cmd_params.vtx_offset as GLint);
                                }
                                DrawCmd::ResetRenderState => { println!("DrawCmd::ResetRenderState."); }
                                DrawCmd::RawCallback {..} => { println!("DrawCmd::RawCallback."); }
                            }
                        }
                        
                        //Free the vertex and index buffers
                        let bufs = [imgui_vao.vbo, imgui_vao.ebo];
                        gl::DeleteBuffers(2, &bufs[0]);
                        gl::DeleteVertexArrays(1, &imgui_vao.vao);
                    }
                }
            }

            //Take a screenshot here as to get the dev gui in it
            if full_screenshot_this_frame {
                take_screenshot(window_size);
                full_screenshot_this_frame = false;
            }
        }

        window.swap_buffers();  //Display the rendered frame to the window
        glfw.poll_events();     //Poll events for next frame
    }
}
