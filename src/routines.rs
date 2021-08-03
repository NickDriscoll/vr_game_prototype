/*
This is a file of miscellaneous functions that used to just live in main.rs
*/

use chrono::offset::Local;
use glfw::{WindowMode};
use gl::types::*;
use image::{ImageBuffer, DynamicImage};
use std::fs;
use std::path::Path;
use std::process::exit;
use std::os::raw::c_void;
use std::sync::mpsc::Sender;
use ozy::{glutil};
use ozy::glutil::ColorSpace;
use ozy::render::{Framebuffer, ScreenState};
use ozy::structs::OptionVec;
use ozy::collision::*;

use crate::structs::*;
use crate::*;

//Saves a screenshot of the current framebuffer to disk
pub unsafe fn screenshot(screen_state: &ScreenState, flag: &mut bool) {
    if *flag {
        let mut buffer = vec![0u8; (screen_state.get_window_size().x * screen_state.get_window_size().y) as usize * 4];
        gl::ReadPixels(0, 0, screen_state.get_window_size().x as GLint, screen_state.get_window_size().y as GLint, gl::RGBA, gl::UNSIGNED_BYTE, buffer.as_mut_slice() as *mut [u8] as *mut c_void);

        let dynamic_image = match ImageBuffer::from_raw(screen_state.get_window_size().x, screen_state.get_window_size().y, buffer) {
            Some(im) => { Some(DynamicImage::ImageRgba8(im).flipv()) }
            None => { 
                println!("Unable to convert raw to image::DynamicImage");
                None
            }
        };

        if let Some(dyn_image) = dynamic_image {
            //Create the screenshot directory if there isn't one
            let screenshot_dir = "screenshots";
            if !Path::new(screenshot_dir).is_dir() {
                if let Err(e) = fs::create_dir(screenshot_dir) {
                    println!("Unable to create screenshot directory: {}", e);
                }
            }

            if let Err(e) = dyn_image.save(format!("{}/{}.png", screenshot_dir, Local::now().format("%F_%H%M%S"))) {
                println!("Error taking screenshot: {}", e);
            }
        }

        *flag = false;
    }
}

pub unsafe fn create_skybox_cubemap(sky_name: &str) -> GLuint {
	let paths = [
		&format!("skyboxes/{}/rt.tga", sky_name),		//Right side
		&format!("skyboxes/{}/lf.tga", sky_name),		//Left side
		&format!("skyboxes/{}/up.tga", sky_name),		//Up side
		&format!("skyboxes/{}/dn.tga", sky_name),		//Down side
		&format!("skyboxes/{}/bk.tga", sky_name),		//Back side
		&format!("skyboxes/{}/ft.tga", sky_name)		//Front side
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
    //gl::TEXTURE_CUBEMAP_POSITIVE_X + i gets you the correct cube face
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
}

pub fn get_clicked_totoro(totoros: &mut OptionVec<Totoro>, click_ray: &Ray) -> Option<(f32, usize)> {
    let mut smallest_t = f32::INFINITY;
    let mut hit_index = None;
    for i in 0..totoros.len() {
        if let Some(tot) = &totoros[i] {
            let tot_sphere = tot.collision_sphere();
            //Translate the ray, such that the test can be performed on a sphere centered at the origin
            //This just simplifies the math
            let test_ray = Ray {
                origin: click_ray.origin - tot_sphere.focus,
                direction: click_ray.direction
            };

            //Compute t
            let d_dot_p = glm::dot(&test_ray.direction, &test_ray.origin);
            let sqrt_body = d_dot_p * d_dot_p - glm::dot(&test_ray.origin, &test_ray.origin) + tot_sphere.radius * tot_sphere.radius;

            //The sqrt body being negative indicates a miss
            if sqrt_body >= 0.0 {
                //Technically this equation is "plus-or-minus" the square root but we want the closest intersection so it's always minus
                let t = glm::dot(&(-test_ray.direction), &test_ray.origin) - f32::sqrt(sqrt_body);
                if t >= 0.0 && t < smallest_t {
                    smallest_t = t;
                    hit_index = Some((smallest_t, i));
                }
            }
        }
    }
    hit_index
}

pub fn kill_totoro(scene_data: &mut SceneData, totoros: &mut OptionVec<Totoro>, totoro_entity_index: usize, selected: &mut Option<usize>, idx: usize) {
    totoros.delete(idx);
    if let Some(i) = selected {
        if *i == idx {
            *selected = None;
        }
    }
    if let Some(ent) = scene_data.opaque_entities.get_mut_element(totoro_entity_index) {
        if let Some(i) = ent.highlighted_item {
            if i == idx {
                ent.highlighted_item = None;
            }
        }
    }
}

//Returns true if the difference between a and b is close enough to zero
pub fn floats_equal(a: f32, b: f32) -> bool {
    let d = a - b;
    d < EPSILON && d > -EPSILON
}

pub fn compile_shader_or_crash(vert: &str, frag: &str) -> GLuint {
    match glutil::compile_program_from_files(vert, frag)  { 
        Ok(program) => { program }
        Err(e) => {
            tfd::message_box_ok("Error compiling OpenGL shader.", &format!("An error occurred while compiling an OpenGL shader:\n\nVert:\t{}\nFrag:\t{}\n\n{}", vert, frag, e), tfd::MessageBoxIcon::Error);
            exit(-1);
        }
    }
}

//Sends the message or prints an error
pub fn send_or_error<T>(s: &Sender<T>, message: T) {
    if let Err(e) = s.send(message) {
        println!("Error sending message to thread: {}", e);
    }
}

pub fn vec_to_array(vec: glm::TVec3<f32>) -> [f32; 3] {    
    [vec.x, vec.y, vec.z]
}

//Sets a flag to a value or unsets the flag if it already is the value
pub fn handle_radio_flag<F: Eq + Default>(current_flag: &mut F, new_flag: F) {
    if *current_flag != new_flag { *current_flag = new_flag; }
    else { *current_flag = F::default(); }
}

pub fn reset_player_position(player: &mut Player) {    
    player.tracking_position = glm::vec3(0.0, 0.0, 3.0);
    player.tracking_velocity = glm::zero();
    player.tracked_segment = LineSegment::zero();
    player.last_tracked_segment = LineSegment::zero();
    player.jumps_remaining = Player::MAX_JUMPS;
    player.movement_state = MoveState::Falling;
}

pub fn resize_main_window(window: &mut Window, framebuffer: &mut Framebuffer, screen_state: &mut ScreenState, size: glm::TVec2<u32>, pos: (i32, i32), window_mode: WindowMode) {    
    framebuffer.size = (size.x as GLsizei, size.y as GLsizei);
    *screen_state = ScreenState::new(glm::vec2(size.x, size.y), glm::identity(), glm::half_pi(), NEAR_DISTANCE, FAR_DISTANCE);
    window.set_monitor(window_mode, pos.0, pos.1, size.x, size.y, Some(144));
}

pub fn write_matrix_to_buffer(buffer: &mut [f32], index: usize, matrix: glm::TMat4<f32>) {
    for k in 0..16 {
        buffer[16 * index + k] = matrix[k];
    }
}

pub fn lerp(start: &glm::TVec3<f32>, end: &glm::TVec3<f32>, t: f32) -> glm::TVec3<f32> {
    start * (1.0 - t) + end * t
}

//Given the mouse's position on the near clipping plane (A) and the camera's origin position (B),
//computes the normalized ray (A - B), expressed in world-space coords
pub fn compute_click_ray(screen_state: &ScreenState, screen_space_mouse: &glm::TVec2<f32>, camera_position: &glm::TVec3<f32>) -> Ray {
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

    let ray_origin = glm::vec3(camera_position.x, camera_position.y, camera_position.z);
    Ray {
        origin: ray_origin,
        direction: glm::normalize(&(glm::vec4_to_vec3(&world_space_mouse) - ray_origin))
    }
}

pub fn rand_binomial() -> f32 {
    rand::random::<f32>() - rand::random::<f32>()
}