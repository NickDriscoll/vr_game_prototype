use std::ptr;
use std::mem::size_of;
use std::os::raw::c_void;
use ozy::io::OzyMesh;
use ozy::render::{RenderTarget, TextureKeeper};
use ozy::structs::OptionVec;
use ozy::glutil::ColorSpace;
use ozy::{glutil};
use tfd::MessageBoxIcon;
use gl::types::*;

pub const NEAR_DISTANCE: f32 = 0.0625;
pub const FAR_DISTANCE: f32 = 1000000.0;
pub const MSAA_SAMPLES: u32 = 8;
pub const SHADOW_CASCADE_COUNT: usize = 6;
pub const TEXTURE_MAP_COUNT: usize = 3;

pub const STANDARD_HIGHLIGHTED_ATTRIBUTE: GLuint = 5;
pub const STANDARD_TRANSFORM_ATTRIBUTE: GLuint = 6;

pub const DEBUG_HIGHLIGHTED_ATTRIBUTE: GLuint = 2;
pub const DEBUG_COLOR_ATTRIBUTE: GLuint = 3;
pub const DEBUG_TRANSFORM_ATTRIBUTE: GLuint = 4;

const CUBE_INDICES_COUNT: GLsizei = 36;

//Represents all of the data necessary to render an object (potentially instanced) that exists in the 3D scene
#[derive(Clone, Debug)]
pub struct RenderEntity {
    pub vao: GLuint,
    pub instanced_buffers: [GLuint; Self::INSTANCED_BUFFERS_COUNT],         //GL names of instanced buffers
    pub index_count: GLint,
    pub active_instances: GLint,
	pub max_instances: usize,
    pub shader: GLuint,
    pub uv_offset: glm::TVec2<f32>,
    pub uv_scale: glm::TVec2<f32>,
    pub textures: [GLuint; TEXTURE_MAP_COUNT],
    pub cast_shadows: bool
}

impl RenderEntity {
    pub const HIGHLIGHTED_BUFFER_INDEX: usize = 0;
    pub const COLOR_BUFFER_INDEX: usize = 1;
    pub const TRANSFORM_BUFFER_INDEX: usize = 2;
    pub const INSTANCED_BUFFERS_COUNT: usize = 3;

    pub fn from_vao(vao: GLuint, program: GLuint, index_count: usize, instances: usize, instanced_attribute: GLuint) -> Self {
        let transform_buffer = unsafe { glutil::create_instanced_transform_buffer(vao, instances, instanced_attribute) };
        RenderEntity {
            vao,
            instanced_buffers: [0; Self::INSTANCED_BUFFERS_COUNT],
            index_count: index_count as GLint,
            active_instances: instances as GLint,
            max_instances: instances,
            shader: program,
            uv_offset: glm::zero(),
            uv_scale: glm::zero(),
            textures: [0; TEXTURE_MAP_COUNT],
            cast_shadows: true
        }
    }

    pub fn from_ozy(path: &str, program: GLuint, instances: usize, instanced_attribute: GLuint, texture_keeper: &mut TextureKeeper, tex_params: &[(GLenum, GLenum)]) -> Self {
        match OzyMesh::load(&path) {
            Some(meshdata) => unsafe {
                let vao = glutil::create_vertex_array_object(&meshdata.vertex_array.vertices, &meshdata.vertex_array.indices, &meshdata.vertex_array.attribute_offsets);

                //GL texture names
                let (mut albedo, mut normal, mut roughness) = (0, 0, 0);

                //We have to load or create a texture based on whether or not this mesh uses solid colors
                if meshdata.colors.len() == 0 {
                    albedo = texture_keeper.fetch_texture(&meshdata.texture_name, "albedo", &tex_params, ColorSpace::Gamma);
                    normal = texture_keeper.fetch_texture(&meshdata.texture_name, "normal", &tex_params, ColorSpace::Linear);
                    roughness = texture_keeper.fetch_texture(&meshdata.texture_name, "roughness", &tex_params, ColorSpace::Linear);
                } else {
                    let tex_params = [
                        (gl::TEXTURE_WRAP_S, gl::REPEAT),
                        (gl::TEXTURE_WRAP_T, gl::REPEAT),
                        (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
                        (gl::TEXTURE_MAG_FILTER, gl::NEAREST)
                    ];

                    //The albedo texture will simply be a one-dimensional array of solid colors
                    //The UV data on the mesh will choose which color goes where
                    gl::GenTextures(1, &mut albedo);
                    gl::BindTexture(gl::TEXTURE_2D, albedo);
                    glutil::apply_texture_parameters(&tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA32F as GLint, (meshdata.colors.len() / 4) as GLint, 1, 0, gl::RGBA, gl::FLOAT, &meshdata.colors[0] as *const f32 as *const c_void);

                    //Normal map
                    gl::GenTextures(1, &mut normal);
                    gl::BindTexture(gl::TEXTURE_2D, normal);
                    glutil::apply_texture_parameters(&tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA32F as GLint, 1, 1, 0, gl::RGBA, gl::FLOAT, &[0.5f32, 0.5, 1.0, 0.0] as *const f32 as *const c_void);

                    //Roughness map
                    gl::GenTextures(1, &mut roughness);
                    gl::BindTexture(gl::TEXTURE_2D, roughness);
                    glutil::apply_texture_parameters(&tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::R32F as GLint, 1, 1, 0, gl::RED, gl::FLOAT, &[0.5f32] as *const f32 as *const c_void);
                }

                let transform_buffer = glutil::create_instanced_transform_buffer(vao, instances, instanced_attribute);
                RenderEntity {
                    vao,
                    instanced_buffers: [0, 0, transform_buffer],
                    index_count: meshdata.vertex_array.indices.len() as GLint,
                    active_instances: instances as GLint,
                    max_instances: instances,
                    shader: program,                    
                    textures: [albedo, normal, roughness],
                    uv_scale: glm::vec2(1.0, 1.0),
                    uv_offset: glm::vec2(0.0, 0.0),
                    cast_shadows: true
                }
            }
            None => {
                tfd::message_box_ok("Error loading OzyMesh", &format!("Unable to load {}", path), MessageBoxIcon::Error);
                panic!("Unable to load OzyMesh: {}", path);
            }
        }
    }

    pub unsafe fn update_single_transform(&mut self, idx: usize, matrix: &glm::TMat4<f32>, attribute_size: usize) {
        gl::BindBuffer(gl::ARRAY_BUFFER, self.instanced_buffers[Self::TRANSFORM_BUFFER_INDEX]);
        gl::BufferSubData(
            gl::ARRAY_BUFFER,
            (attribute_size * idx * size_of::<GLfloat>()) as GLsizeiptr,
            (attribute_size * size_of::<GLfloat>()) as GLsizeiptr,
            &matrix[0] as *const GLfloat as *const c_void
        );
    }

    unsafe fn write_buffer_to_GPU(&mut self, buffer: &[f32], attribute: GLuint, attribute_floats: usize, buffer_name_index: usize) {
        if self.max_instances < self.active_instances as usize {
            let mut b = 0;
            gl::DeleteBuffers(1, &self.instanced_buffers[Self::TRANSFORM_BUFFER_INDEX] as *const u32);
            gl::GenBuffers(1, &mut b);
            self.instanced_buffers[buffer_name_index] = b;
            
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.instanced_buffers[buffer_name_index]);
            glutil::bind_new_transform_buffer(attribute);

            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.active_instances as usize * attribute_floats * size_of::<GLfloat>()) as GLsizeiptr,
                &buffer[0] as *const GLfloat as *const c_void,
                gl::DYNAMIC_DRAW
            );
        } else if buffer.len() > 0 {
            gl::BindBuffer(gl::ARRAY_BUFFER, self.instanced_buffers[buffer_name_index]);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0 as GLsizeiptr,
                (buffer.len() * size_of::<GLfloat>()) as GLsizeiptr,
                &buffer[0] as *const GLfloat as *const c_void
            );
        }
    }

    pub fn update_transform_buffer(&mut self, transforms: &[f32], instanced_attribute: GLuint) {
        //Record the current active instance count
        let new_instances = transforms.len() as GLint / 16 as GLint;
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(transforms, instanced_attribute, 16, Self::TRANSFORM_BUFFER_INDEX); }
    }

    pub fn update_color_buffer(&mut self, colors: &[f32], instanced_attribute: GLuint) {
        //Record the current active instance count
        let new_instances = colors.len() as GLint / 4 as GLint;
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(colors, instanced_attribute, 4, Self::COLOR_BUFFER_INDEX); }
    }

    pub fn update_highlight_buffer(&mut self, bools: &[f32], instanced_attribute: GLuint) {
        //Record the current active instance count
        let new_instances = bools.len() as GLint / 16 as GLint;
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(bools, instanced_attribute, 1, Self::HIGHLIGHTED_BUFFER_INDEX); }
    }
}

pub struct CascadedShadowMap {
    pub rendertarget: RenderTarget,             //Rendertarget for the shadow atlas
    pub program: GLuint,                        //Associated program name
    pub resolution: GLint,                      //The individual cascades are always squares
    pub matrices: [glm::TMat4<f32>; SHADOW_CASCADE_COUNT],
    pub clip_space_distances: [f32; SHADOW_CASCADE_COUNT + 1]
}

impl CascadedShadowMap {
    pub fn new(rendertarget: RenderTarget, program: GLuint, resolution: GLint) -> Self {
        CascadedShadowMap {
            rendertarget,
            program,
            resolution,
            matrices: [glm::identity(); SHADOW_CASCADE_COUNT],
            clip_space_distances: [0.0; SHADOW_CASCADE_COUNT + 1]
        }
    }
}

pub struct SceneData {
    pub fragment_flag: FragmentFlag,
    pub complex_normals: bool,
    pub skybox_cubemap: GLuint,
    pub skybox_vao: GLuint,
    pub skybox_program: GLuint,
    pub sun_pitch: f32,
    pub sun_yaw: f32,
    pub sun_direction: glm::TVec3<f32>,
    pub sun_color: [f32; 3],
    pub sun_shadow_map: CascadedShadowMap,
    pub ambient_strength: f32,
    pub current_time: f32,
    pub opaque_entities: OptionVec<RenderEntity>,
    pub transparent_entities: OptionVec<RenderEntity>
}

impl Default for SceneData {
    fn default() -> Self {
        let rendertarget = unsafe { RenderTarget::new_shadow((0,0)) };
        let sun_shadow_map = CascadedShadowMap {
            rendertarget,
            program: 0,
            resolution: 0,
            matrices: [glm::identity(); SHADOW_CASCADE_COUNT],
            clip_space_distances: [0.0; SHADOW_CASCADE_COUNT + 1],
        };

        SceneData {
            fragment_flag: FragmentFlag::Default,
            complex_normals: true,
            skybox_cubemap: 0,
            skybox_vao: ozy::prims::skybox_cube_vao(),
            skybox_program: 0,
            sun_pitch: 0.0,
            sun_yaw: 0.0,
            sun_direction: glm::zero(),
            sun_color: [1.0, 1.0, 1.0],
            ambient_strength: 0.2,
            sun_shadow_map,
            current_time: 0.0,
            opaque_entities: OptionVec::new(),
            transparent_entities: OptionVec::new()
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum FragmentFlag {
    Default,
    Normals,
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

//This is the function that renders the 3D objects in the scene
pub unsafe fn main_scene(scene_data: &SceneData, view_data: &ViewData) {
    //Main scene rendering
    gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
    gl::BindTexture(gl::TEXTURE_2D, scene_data.sun_shadow_map.rendertarget.texture);

    //Render opaque geometry
    for opt_entity in scene_data.opaque_entities.iter() {        
        render_entity(opt_entity, scene_data, view_data);
    }

    //Skybox rendering
    
	//Compute the view-projection matrix for the skybox (the conversion functions are just there to nullify the translation component of the view matrix)
	//The skybox vertices should be rotated along with the camera, but they shouldn't be translated in order to maintain the illusion
	//that the sky is infinitely far away
    let skybox_view_projection = view_data.projection_matrix * glm::mat3_to_mat4(&glm::mat4_to_mat3(&view_data.view_matrix));

    //Render the skybox
    let sun_c = glm::vec3(scene_data.sun_color[0], scene_data.sun_color[1], scene_data.sun_color[2]);
    gl::UseProgram(scene_data.skybox_program);
    glutil::bind_matrix4(scene_data.skybox_program, "view_projection", &skybox_view_projection);
    glutil::bind_vector3(scene_data.skybox_program, "sun_color", &sun_c);
    gl::BindTexture(gl::TEXTURE_CUBE_MAP, scene_data.skybox_cubemap);
    gl::BindVertexArray(scene_data.skybox_vao);
    gl::DrawElements(gl::TRIANGLES, CUBE_INDICES_COUNT, gl::UNSIGNED_SHORT, ptr::null());

    //Render transparent geometry
    for opt_entity in scene_data.transparent_entities.iter() {
        render_entity(opt_entity, scene_data, view_data);
    }
}

unsafe fn render_entity(opt_entity: &Option<RenderEntity>, scene_data: &SceneData, view_data: &ViewData) {
    let texture_map_names = ["albedo_tex", "normal_tex", "roughness_tex", "shadow_map"];
    let sun_c = glm::vec3(scene_data.sun_color[0], scene_data.sun_color[1], scene_data.sun_color[2]);
    if let Some(entity) = opt_entity {
        let p = entity.shader;
        gl::UseProgram(p);
        glutil::bind_matrix4_array(p, "shadow_matrices", &scene_data.sun_shadow_map.matrices);
        glutil::bind_matrix4(p, "view_projection", &view_data.view_projection);
        glutil::bind_vector3(p, "sun_direction", &scene_data.sun_direction);
        glutil::bind_vector3(p, "sun_color", &sun_c);
        glutil::bind_float(p, "ambient_strength", scene_data.ambient_strength);
        glutil::bind_float(p, "current_time", scene_data.current_time);
        glutil::bind_int(p, "shadow_map", TEXTURE_MAP_COUNT as GLint);
        glutil::bind_int(p, "complex_normals", scene_data.complex_normals as GLint);
        glutil::bind_float_array(p, "cascade_distances", &scene_data.sun_shadow_map.clip_space_distances[1..]);
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

pub unsafe fn cascaded_shadow_map(shadow_map: &CascadedShadowMap, entities: &[Option<RenderEntity>]) {
    shadow_map.rendertarget.framebuffer.bind();
    gl::UseProgram(shadow_map.program);
    for i in 0..SHADOW_CASCADE_COUNT {
        //Configure rendering for this cascade
        glutil::bind_matrix4(shadow_map.program, "view_projection", &shadow_map.matrices[i]);
        gl::Viewport(i as GLint * shadow_map.resolution, 0, shadow_map.resolution, shadow_map.resolution);

        for opt_entity in entities.iter() {
            if let Some(entity) = opt_entity {
                if entity.cast_shadows {
                    gl::BindVertexArray(entity.vao);
                    gl::DrawElementsInstanced(gl::TRIANGLES, entity.index_count, gl::UNSIGNED_SHORT, ptr::null(), entity.active_instances);
                }
            }
        }
    }
}

pub fn compute_shadow_cascade_matrices(shadow_cascade_distances: &[f32; SHADOW_CASCADE_COUNT + 1], shadow_view: &glm::TMat4<f32>, v_mat: &glm::TMat4<f32>, projection: &glm::TMat4<f32>) -> [glm::TMat4<f32>; SHADOW_CASCADE_COUNT] {       
    let mut out_mats = [glm::identity(); SHADOW_CASCADE_COUNT];

    let shadow_from_view = shadow_view * glm::affine_inverse(*v_mat);
    let fovx = f32::atan(1.0 / projection[0]);
    let fovy = f32::atan(1.0 / projection[5]);

    //Loop computes the shadow matrices for this frame
    for i in 0..SHADOW_CASCADE_COUNT {
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

        let projection_depth = 20.0;
        let shadow_projection = glm::ortho(
            min_x, max_x, min_y, max_y, -8.0 * projection_depth, projection_depth * 6.0
        );

        out_mats[i] = shadow_projection * shadow_view;
    }
    out_mats
}