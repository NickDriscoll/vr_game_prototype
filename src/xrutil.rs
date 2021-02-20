#![allow(dead_code)]
use crate::render::SceneData;
use crate::collision::LineSegment;

pub const VALVE_INDEX_INTERACTION_PROFILE: &str =           "/interaction_profiles/valve/index_controller";
pub const HTC_VIVE_INTERACTION_PROFILE: &str =              "/interaction_profiles/htc/vive_controller";

pub const LEFT_GRIP_POSE: &str =                            "/user/hand/left/input/grip/pose";
pub const LEFT_AIM_POSE: &str =                             "/user/hand/left/input/aim/pose";
pub const LEFT_TRIGGER_FLOAT: &str =                        "/user/hand/left/input/trigger/value";
pub const LEFT_STICK_VECTOR2: &str =                        "/user/hand/left/input/thumbstick";
pub const LEFT_TRACKPAD_VECTOR2: &str =                     "/user/hand/left/input/trackpad";
pub const RIGHT_TRACKPAD_CLICK: &str =                      "/user/hand/right/input/trackpad/click";
pub const RIGHT_TRACKPAD_FORCE: &str =                      "/user/hand/right/input/trackpad/force";
pub const RIGHT_TRIGGER_FLOAT: &str =                       "/user/hand/right/input/trigger/value";
pub const RIGHT_GRIP_POSE: &str =                           "/user/hand/right/input/grip/pose";
pub const RIGHT_AIM_POSE: &str =                            "/user/hand/right/input/aim/pose";

pub fn print_pose(pose: xr::Posef) {
    println!("Position: ({}, {}, {})", pose.position.x, pose.position.y, pose.position.z);
}

pub fn pose_to_viewmat(pose: &xr::Posef, tracking_from_world: &glm::TMat4<f32>) -> glm::TMat4<f32> {
    glm::quat_cast(&glm::quat(pose.orientation.x, pose.orientation.y, pose.orientation.z, -pose.orientation.w)) *
    glm::translation(&glm::vec3(-pose.position.x, -pose.position.y, -pose.position.z)) *
    tracking_from_world
}

//Creates a 4x4 homogenous matrix in world space from a pose expressed in tracking space
pub fn pose_to_mat4(pose: &xr::Posef, world_from_tracking: &glm::TMat4<f32>) -> glm::TMat4<f32> {
    world_from_tracking *
    glm::translation(&glm::vec3(pose.position.x, pose.position.y, pose.position.z)) *
    glm::quat_cast(&glm::quat(pose.orientation.x, pose.orientation.y, pose.orientation.z, pose.orientation.w))
}

pub fn make_path(instance: &Option<xr::Instance>, path_string: &str) -> Option<xr::Path> {
    match instance {
        Some(inst) => {
            match inst.string_to_path(path_string) {
                Ok(path) => { Some(path) }
                Err(e) => {
                    println!("Error getting XrPath: {}", e);
                    None
                }
            }
        }
        None => { None }
    }    
}

pub fn make_action<T: xr::ActionTy>(subaction_path: &Option<xr::Path>, actionset: &Option<xr::ActionSet>, name: &str, localized_name: &str) -> Option<xr::Action<T>> {
    match (subaction_path, actionset) {
        (Some(path), Some(actionset)) => {
            match actionset.create_action::<T>(name, localized_name, &[*path]) {
                Ok(action) => { Some(action) }
                Err(e) => {
                    println!("Error creating XrAction: {}", e);
                    None
                }
            }
        }
        _ => { None }
    }
}

pub fn get_actionstate<G: xr::Graphics, T: xr::ActionInput>(xr_session: &Option<xr::Session<G>>, xr_action: &Option<xr::Action<T>>) -> Option<xr::ActionState<T>> {
    match (xr_session, xr_action) {
        (Some(session), Some(action)) => {
            match action.state(session, xr::Path::NULL) {
                Ok(state) => { Some(state) }
                Err(e) => {
                    println!("{}", e);
                    None
                }
            }
        }
        _ => { None }
    }
}

pub fn make_actionspace<G: xr::Graphics>(session: &Option<xr::Session<G>>, subaction_path: Option<xr::Path>, pose_action: &Option<xr::Action<xr::Posef>>, reference_pose: xr::Posef) -> Option<xr::Space> {
    match (session, subaction_path, pose_action) {
        (Some(session), Some(path), Some(action)) => {
            match action.create_space(session.clone(), path, reference_pose) {
                Ok(space) => { Some(space) }
                Err(e) => {
                    println!("Couldn't get left hand space: {}", e);
                    None
                }
            }
        }
        _ => { None }
    }
}

pub fn locate_space(space: &Option<xr::Space>, tracking_space: &Option<xr::Space>, time: xr::Time) -> Option<xr::Posef> {
    match (space, tracking_space) {
        (Some(space), Some(t_space)) => {
            match space.locate(t_space, time) {
                Ok(space_location) => {
                    Some(space_location.pose)
                }
                Err(e) => {
                    println!("Couldn't locate space: {}", e);
                    None
                }
            }
        
        }
        _ => { None }
    }
}

pub fn make_reference_space<G: xr::Graphics>(session: &Option<xr::Session<G>>, ref_type: xr::ReferenceSpaceType, pose_in_ref_space: xr::Posef) -> Option<xr::Space> {
    match session {
        Some(sess) => {
            match sess.create_reference_space(ref_type, pose_in_ref_space) {
                Ok(space) => { Some(space) }
                Err(e) => {
                    println!("Couldn't create reference space: {}", e);
                    None
                }
            }
        }
        None => { None }
    }
}


pub fn entity_pose_update(scene_data: &mut SceneData, entity_index: usize, pose: Option<xr::Posef>, world_from_tracking: &glm::TMat4<f32>) {
    if let Some(p) = pose {
        scene_data.single_entities[entity_index].model_matrix = pose_to_mat4(&p, world_from_tracking);
    }
}

pub fn tracked_player_segment(view_space: &Option<xr::Space>, tracking_space: &Option<xr::Space>, time: xr::Time, world_from_tracking: &glm::TMat4<f32>) -> LineSegment {
    match locate_space(&view_space, &tracking_space, time) {
        Some(pose) => {
            let head = world_from_tracking * glm::vec4(pose.position.x, pose.position.y, pose.position.z, 1.0);
            let feet = world_from_tracking * glm::vec4(pose.position.x, pose.position.y, 0.0, 1.0);
            LineSegment {
                p0: head,
                p1: feet
            }
        }
        None => { LineSegment {p0: glm::zero(), p1: glm::zero()} }
    }
}

/*
unsafe extern "system" fn debug_callback(severity_flags: DebugUtilsMessageSeverityFlagsEXT, type_flags: DebugUtilsMessageTypeFlagsEXT, callback_data: *const DebugUtilsMessengerCallbackDataEXT, user_data: *mut c_void) -> Bool32 {
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
*/