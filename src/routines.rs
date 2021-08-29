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

use crate::traits::PositionAble;
use crate::traits::Spherical;
use crate::gamestate::*;
use crate::structs::*;
use crate::*;

//Saves a screenshot of the current framebuffer to disk
pub unsafe fn take_screenshot(screen_state: &ScreenState) {
    let mut buffer = vec![0u8; (screen_state.get_window_size().x * screen_state.get_window_size().y) as usize * 4];
    gl::ReadPixels(
        0,
        0,
        screen_state.get_window_size().x as GLint,
        screen_state.get_window_size().y as GLint,
        gl::RGBA,
        gl::UNSIGNED_BYTE,
        buffer.as_mut_slice() as *mut [u8] as *mut c_void
    );

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
	gl::TexParameteri(gl::TEXTURE_CUBE_MAP, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);

	//Place each piece of the skybox on the correct face
    //gl::TEXTURE_CUBEMAP_POSITIVE_X + i gets you the correct cube face
	for i in 0..6 {
        let target = gl::TEXTURE_CUBE_MAP_POSITIVE_X + i as u32;
		let image_data = glutil::image_data_from_path(paths[i], ColorSpace::Gamma);
		gl::TexImage2D(target,
					   0,
					   image_data.internal_format as i32,
					   image_data.width as i32,
					   image_data.height as i32,
					   0,
					   image_data.format,
					   gl::UNSIGNED_BYTE,
			  		   &image_data.data[0] as *const u8 as *const c_void);
	}
    gl::GenerateMipmap(gl::TEXTURE_CUBE_MAP);
	cubemap
}

pub fn get_clicked_object<T: Spherical>(objects: &OptionVec<T>, click_ray: &Ray) -> Option<(f32, usize)> {
    let mut smallest_t = f32::INFINITY;
    let mut hit_index = None;
    for i in 0..objects.len() {
        if let Some(thing) = &objects[i] {
            let sphere = thing.sphere();
            //Translate the ray, such that the test can be performed on a sphere centered at the origin
            //This just simplifies the math
            let test_ray = Ray {
                origin: click_ray.origin - sphere.focus,
                direction: click_ray.direction
            };

            //Compute t
            let d_dot_p = glm::dot(&test_ray.direction, &test_ray.origin);
            let sqrt_body = d_dot_p * d_dot_p - glm::dot(&test_ray.origin, &test_ray.origin) + sphere.radius * sphere.radius;

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

pub fn move_object<T: PositionAble>(objects: &mut OptionVec<T>, terrain: &Terrain, scene_data: &SceneData, camera: &Camera, mouse: &Mouse) {
    if let Some(idx) = scene_data.selected_point_light {
        let click_ray = compute_click_ray(&camera.screen_state, &mouse.screen_space_pos, &camera.position);
        if let Some((_, point)) = ray_hit_terrain(terrain, &click_ray) {
            if let Some(ob) = objects.get_mut_element(idx) {
                ob.set_position(point + glm::vec3(0.0, 0.0, 2.0));
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
pub fn do_radio_button<F: Eq + Default>(imgui_ui: &imgui::Ui, label: &imgui::ImStr, flag: &mut F, new_flag: F) {
    if imgui_ui.radio_button_bool(label, *flag == new_flag) { 
        if *flag != new_flag { *flag = new_flag; }
        else { *flag = F::default(); }
    }
}

pub fn resize_main_window(window: &mut Window, framebuffer: &mut Framebuffer, screen_state: &mut ScreenState, size: glm::TVec2<u32>, pos: (i32, i32), window_mode: WindowMode) {    
    framebuffer.size = (size.x as GLsizei, size.y as GLsizei);
    *screen_state = ScreenState::new(glm::vec2(size.x, size.y), glm::identity(), glm::half_pi(), NEAR_DISTANCE, FAR_DISTANCE);
    window.set_monitor(window_mode, pos.0, pos.1, size.x, size.y, Some(144));
}

pub fn write_vec4_to_buffer(buffer: &mut [f32], index: usize, vec: glm::TVec4<f32>) {
    buffer[4 * index + 0] = vec.x;
    buffer[4 * index + 1] = vec.y;
    buffer[4 * index + 2] = vec.z;
    buffer[4 * index + 3] = vec.w;
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
    let fovy_radians = screen_state.get_fov_radians();
    let fovx_radians = 2.0 * f32::atan(f32::tan(fovy_radians / 2.0) * screen_state.get_aspect_ratio());
    let max_coords = glm::vec4(
        NEAR_DISTANCE * f32::tan(fovx_radians / 2.0),
        NEAR_DISTANCE * f32::tan(fovy_radians / 2.0),
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

pub fn load_lvl(level_name: &str, world_state: &mut WorldState, scene_data: &mut SceneData, texture_keeper: &mut TextureKeeper, standard_program: GLuint) {    
    let level_load_error = |s: std::io::Error| {
        tfd::message_box_ok("Error loading level", &format!("Error reading from level {}: {}", level_name, s), MessageBoxIcon::Error);
        exit(-1);
    };

    world_state.level_name = String::from(level_name);

    let index_arrays = [&mut world_state.opaque_terrain_indices, &mut world_state.transparent_terrain_indices];
    let entity_arrays = [&mut scene_data.opaque_entities, &mut scene_data.transparent_entities];
    for i in 0..index_arrays.len() {
        for index in index_arrays[i].iter() {
            entity_arrays[i].delete(*index);
        }
        index_arrays[i].clear();
    }

    //Load the scene data from the level file
    match File::open(&format!("maps/{}.lvl", world_state.level_name)) {
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
                        level_load_error(e)
                    }
                };

                //Read number of matrices
                let matrices_count = match io::read_u32(&mut file) {
                    Ok(count) => { count as usize } 
                    Err(e) => {
                        tfd::message_box_ok("Error loading level", &format!("Error reading from level {}: {}", world_state.level_name, e), MessageBoxIcon::Error);
                        panic!("Error reading from level file: {}", e);
                    }
                };
                let matrix_floats = match io::read_f32_data(&mut file, matrices_count as usize * 16) {
                    Ok(floats) => { floats }
                    Err(e) => {
                        tfd::message_box_ok("Error loading level", &format!("Error reading from level {}: {}", world_state.level_name, e), MessageBoxIcon::Error);
                        panic!("Error reading from level file: {}", e);
                    }
                };

                let mut entity = RenderEntity::from_ozy(&format!("models/{}", ozy_name), standard_program, matrices_count, STANDARD_TRANSFORM_ATTRIBUTE, texture_keeper, &DEFAULT_TEX_PARAMS);
                entity.update_transform_buffer(&matrix_floats, STANDARD_TRANSFORM_ATTRIBUTE);

                if entity.transparent {                    
                    world_state.transparent_terrain_indices.push(scene_data.transparent_entities.insert(entity));
                } else {
                    world_state.opaque_terrain_indices.push(scene_data.opaque_entities.insert(entity));
                }                
            }                
        }
        Err(e) => { level_load_error(e); }
    }
}

pub fn load_ent(path: &str, scene_data: &mut SceneData, world_state: &mut WorldState) {
    fn io_or_error<T>(res: Result<T, std::io::Error>, level_name: &str) -> T {
        match res {
            Ok(r) => { r }
            Err(e) => {
                tfd::message_box_ok("Error loading level", &format!("Error reading from level {}: {}\n", level_name, e), MessageBoxIcon::Error);
                panic!("Error reading from level file: {}", e);
            }
        }
    }

    //First, clear world data
    world_state.totoros.clear();
    scene_data.point_lights.clear();
    world_state.selected_totoro = None;
    scene_data.selected_point_light = None;

    match File::open(path) {
        Ok(mut file) => {
            let floats_at_start = 12;
            let r = io::read_pascal_strings(&mut file, 1);
            let new_skybox = io_or_error(r, path)[0].clone();

            let raw_floats = io_or_error(io::read_f32_data(&mut file, floats_at_start), path);

            scene_data.ambient_strength = raw_floats[0];
            scene_data.sun_pitch = raw_floats[1];
            scene_data.sun_yaw = raw_floats[2];
            scene_data.sun_color[0] = raw_floats[3];
            scene_data.sun_color[1] = raw_floats[4];
            scene_data.sun_color[2] = raw_floats[5];
            scene_data.shininess_lower_bound = raw_floats[6];
            scene_data.shininess_upper_bound = raw_floats[7];
            scene_data.sun_size = raw_floats[8];
            world_state.player_spawn.x = raw_floats[9];
            world_state.player_spawn.y = raw_floats[10];
            world_state.player_spawn.z = raw_floats[11];
            
            //Load totoros
            let floats_per_totoro = 4;
            let totoros_count = io_or_error(io::read_u32(&mut file), path);                
            let raw_floats = io_or_error(io::read_f32_data(&mut file, totoros_count as usize * floats_per_totoro), path);
            for i in (0..raw_floats.len()).step_by(floats_per_totoro) {
                let pos = glm::vec3(raw_floats[i], raw_floats[i + 1], raw_floats[i + 2]);                
                let mut tot = Totoro::new(pos, rand::random::<f32>() * 4.5 - 2.0);
                tot.scale = raw_floats[i + 3];
                world_state.totoros.insert(tot);
            }

            //Load lights
            let floats_per_light = 7;
            let light_count = io_or_error(io::read_u32(&mut file), path);
            let raw_floats = io_or_error(io::read_f32_data(&mut file, light_count as usize * floats_per_light), path);
            for i in (0..raw_floats.len()).step_by(floats_per_light) {                
                let position = glm::vec3(raw_floats[i], raw_floats[i + 1], raw_floats[i + 2]);    
                let color = [raw_floats[i + 3], raw_floats[i + 4], raw_floats[i + 5]];
                let power = raw_floats[i + 6];
                let light = PointLight {
                    position,
                    color,
                    power
                };

                scene_data.point_lights.insert(light);
            }


            //Create the skybox cubemap
            scan_skybox_directory(world_state, &new_skybox);
            scene_data.skybox_cubemap = unsafe { 
                gl::DeleteTextures(1, &mut scene_data.skybox_cubemap);
                create_skybox_cubemap(world_state.skybox_strings[world_state.active_skybox_index].to_str())
            };
        }
        Err(e) => {
            tfd::message_box_ok("Error loading level data", &format!("Could not load level data:\n{}\nHave you saved the level data for this level yet?", e), MessageBoxIcon::Error);

            //We still want the skybox strings to get recomputed even if we can't load the ent file
            scan_skybox_directory(world_state, "");
        }
    }
}

fn scan_skybox_directory(world_state: &mut WorldState, skybox_name: &str) {
    world_state.active_skybox_index = 0;
    world_state.skybox_strings = {
        let mut v = Vec::new();
        match read_dir("skyboxes/") {
            Ok(iter) => {
                let mut current_skybox = 0;
                for entry in iter {
                    match entry {
                        Ok(ent) => {
                            let name = ent.file_name().into_string().unwrap();
                            if name == skybox_name {
                                world_state.active_skybox_index = current_skybox;
                            }
                            v.push(im_str!("{}", name));
                        }
                        Err(e) => {
                            tfd::message_box_ok("Unable to read skybox entry", &format!("{}", e), MessageBoxIcon::Error);
                        }
                    }
                    current_skybox += 1;
                }
            }
            Err(e) => {
                tfd::message_box_ok("Unable to read skybox directory", &format!("{}", e), MessageBoxIcon::Error);
            }
        }
        v
    };
}