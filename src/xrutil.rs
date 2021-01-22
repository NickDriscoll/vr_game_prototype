#![allow(dead_code)]

pub fn print_pose(pose: xr::Posef) {
    println!("Position: ({}, {}, {})", pose.position.x, pose.position.y, pose.position.z);
}

pub fn pose_to_viewmat(pose: &xr::Posef, tracking_from_world: &glm::TMat4<f32>) -> glm::TMat4<f32> {
    glm::quat_cast(&glm::quat(pose.orientation.x, pose.orientation.y, pose.orientation.z, -pose.orientation.w)) *
    glm::translation(&glm::vec3(-pose.position.x, -pose.position.y, -pose.position.z)) *
    tracking_from_world
}

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