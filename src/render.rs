use std::ptr;
use std::mem::size_of;
use std::os::raw::c_void;
use ozy::io::OzyMesh;
use ozy::render::{TextureKeeper};
use ozy::structs::OptionVec;
use ozy::glutil::ColorSpace;
use crate::glutil;
use gl::types::*;

pub const NEAR_DISTANCE: f32 = 0.0625;
pub const FAR_DISTANCE: f32 = 1000000.0;
pub const MSAA_SAMPLES: u32 = 8;
pub const SHADOW_CASCADES: usize = 6;
pub const INSTANCED_ATTRIBUTE: GLuint = 5;
pub const TEXTURE_MAP_COUNT: usize = 3;

//Represents all of the data necessary to render an object that exists in the 3D scene
pub struct RenderEntity {
    pub vao: GLuint,
    pub transform_buffer: GLuint,       //GPU buffer with one 4x4 homogenous transform per instance
    pub index_count: GLint,
    pub active_instances: GLint,
	pub max_instances: usize,
    pub shader: GLuint,
    pub uv_offset: glm::TVec2<f32>,
    pub uv_scale: glm::TVec2<f32>,
    pub textures: [GLuint; TEXTURE_MAP_COUNT],
    pub color: glm::TVec3<f32>
}

impl RenderEntity {
    pub fn from_ozy(path: &str, program: GLuint, instances: usize, texture_keeper: &mut TextureKeeper, tex_params: &[(GLenum, GLenum)]) -> Self {
        match OzyMesh::load(&path) {
            Some(meshdata) => unsafe {
                let vao = glutil::create_vertex_array_object(&meshdata.vertex_array.vertices, &meshdata.vertex_array.indices, &meshdata.vertex_array.attribute_offsets);
                let albedo = texture_keeper.fetch_texture(&meshdata.texture_name, "albedo", &tex_params, ColorSpace::Gamma);
                let normal = texture_keeper.fetch_texture(&meshdata.texture_name, "normal", &tex_params, ColorSpace::Linear);
                let roughness = texture_keeper.fetch_texture(&meshdata.texture_name, "roughness", &tex_params, ColorSpace::Linear);

                let transform_buffer = glutil::create_instanced_transform_buffer(vao, instances, INSTANCED_ATTRIBUTE);
                RenderEntity {
                    vao,
                    transform_buffer,
                    index_count: meshdata.vertex_array.indices.len() as GLint,
                    active_instances: instances as GLint,
                    max_instances: instances,
                    shader: program,                    
                    textures: [albedo, normal, roughness],
                    uv_scale: glm::vec2(1.0, 1.0),
                    uv_offset: glm::vec2(0.0, 0.0),
                    color: glm::zero()
                }
            }
            None => {
                panic!("Unable to load OzyMesh: {}", path);
            }
        }
    }

    pub unsafe fn update_single_transform(&mut self, idx: usize, matrix: &glm::TMat4<f32>) {
        gl::BindBuffer(gl::ARRAY_BUFFER, self.transform_buffer);
        gl::BufferSubData(gl::ARRAY_BUFFER, (16 * idx * size_of::<GLfloat>()) as GLsizeiptr, (16 * size_of::<GLfloat>()) as GLsizeiptr, &matrix[0] as *const GLfloat as *const c_void);
    }

    pub fn update_buffer(&mut self, transforms: &[f32]) {
        //Record the current active instance count
        self.active_instances = transforms.len() as GLint / 16;

        //Update GPU buffer storing hit volume transforms
		if transforms.len() > 0 {
			unsafe {
				gl::BindBuffer(gl::ARRAY_BUFFER, self.transform_buffer);
				gl::BufferSubData(gl::ARRAY_BUFFER,
								0 as GLsizeiptr,
								(transforms.len() * size_of::<GLfloat>()) as GLsizeiptr,
								&transforms[0] as *const GLfloat as *const c_void
								);
			}
		}
    }
}

pub struct CascadedShadowMap {
    pub texture: GLuint,
    pub program: GLuint,
    pub resolution: GLint,          //The assumption is that the individual cascades are always squares
    pub matrices: [glm::TMat4<f32>; SHADOW_CASCADES],
    pub clip_space_distances: [f32; SHADOW_CASCADES + 1]
}

impl CascadedShadowMap {
    pub fn new(texture: GLuint, program: GLuint, resolution: GLint) -> Self {
        CascadedShadowMap {
            texture,
            program,
            resolution,
            matrices: [glm::identity(); SHADOW_CASCADES],
            clip_space_distances: [0.0; SHADOW_CASCADES + 1]
        }
    }
}

pub struct SceneData {
    pub fragment_flag: FragmentFlag,
    pub complex_normals: bool,
    pub skybox_cubemap: GLuint,
    pub skybox_vao: GLuint,
    pub skybox_program: GLuint,
    pub sun_direction: glm::TVec3<f32>,
    pub sun_color: [f32; 3],
    pub ambient_strength: f32,
    pub sun_shadow_map: CascadedShadowMap,
    pub entities: OptionVec<RenderEntity>
}

impl Default for SceneData {
    fn default() -> Self {
        let sun_shadow_map = CascadedShadowMap {
            texture: 0,
            program: 0,
            resolution: 0,
            matrices: [glm::identity(); SHADOW_CASCADES],
            clip_space_distances: [0.0; SHADOW_CASCADES + 1],
        };

        SceneData {
            fragment_flag: FragmentFlag::Default,
            complex_normals: true,
            skybox_cubemap: 0,
            skybox_vao: 0,
            skybox_program: 0,
            sun_direction: glm::normalize(&glm::vec3(1.0, 0.6, 1.0)),
            sun_color: [1.0, 1.0, 1.0],
            ambient_strength: 0.2,
            sun_shadow_map,
            entities: OptionVec::new()
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum FragmentFlag {
    Default,
    Normals,
    LodZones,
    CascadeZones,
    Shadowed
}

impl Default for FragmentFlag {
    fn default() -> Self {
        FragmentFlag::Default
    }
}

pub struct ViewData {
    pub view_position: glm::TVec3<f32>,
    pub view_matrix: glm::TMat4<f32>,
    pub projection_matrix: glm::TMat4<f32>,
    pub view_projection: glm::TMat4<f32>
}

impl ViewData {
    pub fn new(view_position: glm::TVec3<f32>, view_matrix: glm::TMat4<f32>, projection_matrix: glm::TMat4<f32>) -> Self {
        Self {
            view_position,
            view_matrix,
            projection_matrix,
            view_projection: projection_matrix * view_matrix
        }
    }
}

//This is the function that renders the image you would actually see on screen/in HMD
pub unsafe fn render_main_scene(scene_data: &SceneData, view_data: &ViewData) {
    let texture_map_names = ["albedo_map", "normal_map", "roughness_map", "shadow_map"];
    let sun_shadow_map = &scene_data.sun_shadow_map;

    //Main scene rendering
    gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
    gl::BindTexture(gl::TEXTURE_2D, scene_data.sun_shadow_map.texture);

    //Render 3D entities
    let sun_c = glm::vec3(scene_data.sun_color[0], scene_data.sun_color[1], scene_data.sun_color[2]);
    for opt_entity in scene_data.entities.iter() {
        if let Some(entity) = opt_entity {
            let p = entity.shader;
            gl::UseProgram(p);
            glutil::bind_matrix4_array(p, "shadow_matrices", &sun_shadow_map.matrices);
            glutil::bind_matrix4(p, "view_projection", &view_data.view_projection);
            glutil::bind_vector3(p, "sun_direction", &scene_data.sun_direction);
            glutil::bind_vector3(p, "sun_color", &sun_c);
            glutil::bind_float(p, "ambient_strength", scene_data.ambient_strength);
            glutil::bind_int(p, "shadow_map", TEXTURE_MAP_COUNT as GLint);
            glutil::bind_int(p, "complex_normals", scene_data.complex_normals as GLint);
            glutil::bind_float_array(p, "cascade_distances", &sun_shadow_map.clip_space_distances[1..]);
            glutil::bind_vector3(p, "view_position", &view_data.view_position);
            glutil::bind_vector2(p, "uv_scale", &entity.uv_scale);
            glutil::bind_vector2(p, "uv_offset", &entity.uv_offset);

            //fragment flag stuff
            let flag_names = ["visualize_normals", "visualize_lod", "visualize_shadowed", "visualize_cascade_zone"];
            for name in flag_names.iter() {
                glutil::bind_int(p, name, 0);
            }
            match scene_data.fragment_flag {
                FragmentFlag::Shadowed => { glutil::bind_int(p, "visualize_shadowed", 1); }
                FragmentFlag::Normals => { glutil::bind_int(p, "visualize_normals", 1); }
                FragmentFlag::LodZones => { glutil::bind_int(p, "visualize_lod", 1); }
                FragmentFlag::CascadeZones => { glutil::bind_int(p, "visualize_cascade_zone", 1); }
                FragmentFlag::Default => {}
            }
            
            for i in 0..TEXTURE_MAP_COUNT {
                glutil::bind_int(p, texture_map_names[i], i as GLint);
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, entity.textures[i]);
            }            

            gl::BindVertexArray(entity.vao);
            gl::DrawElementsInstanced(gl::TRIANGLES, entity.index_count, gl::UNSIGNED_SHORT, ptr::null(), entity.active_instances);
        }
    }

    //Skybox rendering
    
	//Compute the view-projection matrix for the skybox (the conversion functions are just there to nullify the translation component of the view matrix)
	//The skybox vertices should obviously be rotated along with the camera, but they shouldn't be translated in order to maintain the illusion
	//that the sky is infinitely far away
    let skybox_view_projection = view_data.projection_matrix * glm::mat3_to_mat4(&glm::mat4_to_mat3(&view_data.view_matrix));

    //Render the skybox
    gl::UseProgram(scene_data.skybox_program);
    glutil::bind_matrix4(scene_data.skybox_program, "view_projection", &skybox_view_projection);
    glutil::bind_vector3(scene_data.skybox_program, "sun_color", &sun_c);
    gl::BindTexture(gl::TEXTURE_CUBE_MAP, scene_data.skybox_cubemap);
    gl::BindVertexArray(scene_data.skybox_vao);
    gl::DrawElements(gl::TRIANGLES, 36, gl::UNSIGNED_SHORT, ptr::null());
}

pub unsafe fn render_cascaded_shadow_map(shadow_map: &CascadedShadowMap, entities: &[Option<RenderEntity>]) {
    gl::UseProgram(shadow_map.program);
    for i in 0..SHADOW_CASCADES {
        //Configure rendering for this cascade
        glutil::bind_matrix4(shadow_map.program, "view_projection", &shadow_map.matrices[i]);
        gl::Viewport(i as GLint * shadow_map.resolution, 0, shadow_map.resolution, shadow_map.resolution);

        for opt_entity in entities.iter() {
            if let Some(entity) = opt_entity {
                gl::BindVertexArray(entity.vao);
                gl::DrawElementsInstanced(gl::TRIANGLES, entity.index_count, gl::UNSIGNED_SHORT, ptr::null(), entity.active_instances);
            }
        }
    }
}

pub fn compute_shadow_cascade_matrices(shadow_cascade_distances: &[f32; SHADOW_CASCADES + 1], shadow_view: &glm::TMat4<f32>, v_mat: &glm::TMat4<f32>, projection: &glm::TMat4<f32>) -> [glm::TMat4<f32>; SHADOW_CASCADES] {       
    let mut out_mats = [glm::identity(); SHADOW_CASCADES];

    let shadow_from_view = shadow_view * glm::affine_inverse(*v_mat);
    let fovx = f32::atan(1.0 / projection[0]);
    let fovy = f32::atan(1.0 / projection[5]);

    //Loop computes the shadow matrices for this frame
    for i in 0..SHADOW_CASCADES {
        //Near and far distances for this sub-frustum
        let z0 = shadow_cascade_distances[i];
        let z1 = shadow_cascade_distances[i + 1];

        //Computing the view-space coords of the sub-frustum vertices
        let x0 = -z0 * f32::tan(fovx);
        let x1 = z0 * f32::tan(fovx);
        let x2 = -z1 * f32::tan(fovx);
        let x3 = z1 * f32::tan(fovx);
        let y0 = -z0 * f32::tan(fovy);
        let y1 = z0 * f32::tan(fovy);
        let y2 = -z1 * f32::tan(fovy);
        let y3 = z1 * f32::tan(fovy);

        //The extreme vertices of the sub-frustum
        let shadow_space_points = [
            shadow_from_view * glm::vec4(x0, y0, z0, 1.0),
            shadow_from_view * glm::vec4(x1, y0, z0, 1.0),
            shadow_from_view * glm::vec4(x0, y1, z0, 1.0),
            shadow_from_view * glm::vec4(x1, y1, z0, 1.0),                                        
            shadow_from_view * glm::vec4(x2, y2, z1, 1.0),
            shadow_from_view * glm::vec4(x3, y2, z1, 1.0),
            shadow_from_view * glm::vec4(x2, y3, z1, 1.0),
            shadow_from_view * glm::vec4(x3, y3, z1, 1.0)                                        
        ];

        //Determine the boundaries of the orthographic projection
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = 0.0;
        let mut max_y = 0.0;
        for point in shadow_space_points.iter() {
            if max_x < point.x { max_x = point.x; }
            if min_x > point.x { min_x = point.x; }
            if max_y < point.y { max_y = point.y; }
            if min_y > point.y { min_y = point.y; }
        }

        let projection_depth = 10.0;
        let shadow_projection = glm::ortho(
            min_x, max_x, min_y, max_y, -10.0 * projection_depth, projection_depth * 4.0
        );

        out_mats[i] = shadow_projection * shadow_view;
    }
    out_mats
}