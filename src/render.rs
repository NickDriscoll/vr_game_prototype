use std::ptr;
use std::mem::size_of;
use std::os::raw::c_void;
use ozy::collision::Sphere;
use ozy::io::OzyMesh;
use ozy::render::{Framebuffer, RenderTarget, TextureKeeper};
use ozy::structs::OptionVec;
use ozy::glutil::{ColorSpace, VertexArrayNames};
use ozy::{glutil};
use tfd::MessageBoxIcon;
use gl::types::*;
use crate::traits::SphereCollider;

pub const NEAR_DISTANCE: f32 = 0.0625;
pub const FAR_DISTANCE: f32 = 1_000.0;
pub const MSAA_SAMPLES: u32 = 8;
pub const SHADOW_CASCADE_COUNT: usize = 5;
pub const ENTITY_TEXTURE_COUNT: usize = 3;
pub const MAX_POINT_LIGHTS: usize = 8;
pub const POINT_LIGHTS_BINDING_POINT: GLuint = 1;

pub const STANDARD_HIGHLIGHTED_ATTRIBUTE: GLuint = 5;
pub const STANDARD_TRANSFORM_ATTRIBUTE: GLuint = 6;

pub const DEBUG_HIGHLIGHTED_ATTRIBUTE: GLuint = 2;
pub const DEBUG_COLOR_ATTRIBUTE: GLuint = 3;
pub const DEBUG_TRANSFORM_ATTRIBUTE: GLuint = 4;

const CUBE_INDICES_COUNT: GLsizei = 36;

//Represents all of the data necessary to render an object (potentially instanced) that exists in the 3D scene
#[derive(Clone, Debug)]
pub struct RenderEntity {
    pub vao: VertexArrayNames,
    pub instanced_buffers: [GLuint; Self::INSTANCED_BUFFERS_COUNT],         //GL names of instanced buffers
    pub index_count: GLint,
    pub active_instances: usize,
    pub shader: GLuint,
    pub uv_velocity: glm::TVec2<f32>,
    pub uv_offset: glm::TVec2<f32>,
    pub uv_scale: glm::TVec2<f32>,
    pub material_textures: [GLuint; ENTITY_TEXTURE_COUNT],      //The 2D maps that define a material
    pub using_cached_textures: bool,
    pub lookup_texture: GLuint,                                 //A 1D lookup texture whose use is defined by the shader
    pub transparent: bool,
    pub ignore_depth: bool                                      //Ignore depth testing for this entity
}

impl RenderEntity {
    pub const HIGHLIGHTED_BUFFER_INDEX: usize = 0;
    pub const COLOR_BUFFER_INDEX: usize = 1;
    pub const TRANSFORM_BUFFER_INDEX: usize = 2;
    pub const INSTANCED_BUFFERS_COUNT: usize = 3;

    pub fn from_vao(vao: VertexArrayNames, program: GLuint, index_count: usize, instances: usize, instanced_attribute: GLuint, using_cached_textures: bool) -> Self {
        let transform_buffer = unsafe { glutil::create_instanced_transform_buffer(vao.vao, instances, instanced_attribute) };
        RenderEntity {
            vao,
            instanced_buffers: [0, 0, transform_buffer],
            index_count: index_count as GLint,
            active_instances: instances,
            shader: program,
            uv_velocity: glm::zero(),
            uv_offset: glm::zero(),
            uv_scale: glm::zero(),
            material_textures: [0; ENTITY_TEXTURE_COUNT],
            using_cached_textures,
            lookup_texture: 0,
            transparent: false,
            ignore_depth: false
        }
    }

    pub fn from_ozy(path: &str, program: GLuint, instances: usize, instanced_attribute: GLuint, texture_keeper: &mut TextureKeeper, tex_params: &[(GLenum, GLenum)]) -> Self {
        match OzyMesh::load(&path) {
            Some(meshdata) => unsafe {
                let vao = glutil::create_vertex_array_object(&meshdata.vertex_array.vertices, &meshdata.vertex_array.indices, &meshdata.vertex_array.attribute_offsets);

                let mut textures = [0; 3];  //[albedo, normal, roughness]

                //We have to load or create a texture based on whether or not this mesh uses solid colors
                let using_cached_textures;
                if meshdata.colors.len() == 0 {
                    textures[0] = texture_keeper.fetch_material(&meshdata.texture_name, "albedo", &tex_params, ColorSpace::Gamma);
                    textures[1] = texture_keeper.fetch_material(&meshdata.texture_name, "normal", &tex_params, ColorSpace::Linear);
                    textures[2] = texture_keeper.fetch_material(&meshdata.texture_name, "roughness", &tex_params, ColorSpace::Linear);
                    using_cached_textures = true;
                } else {
                    let simple_tex_params = [
                        (gl::TEXTURE_WRAP_S, gl::REPEAT),
                        (gl::TEXTURE_WRAP_T, gl::REPEAT),
                        (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
                        (gl::TEXTURE_MAG_FILTER, gl::NEAREST)
                    ];

                    //Gen the three textures
                    gl::GenTextures(3, &mut textures[0]);

                    //The albedo texture will simply be a one-dimensional array of solid colors
                    //The UV data on the mesh will choose which color goes where
                    gl::BindTexture(gl::TEXTURE_2D, textures[0]);
                    glutil::apply_texture_parameters(gl::TEXTURE_2D, &simple_tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA8 as GLint, (meshdata.colors.len() / 4) as GLint, 1, 0, gl::RGBA, gl::FLOAT, &meshdata.colors[0] as *const f32 as *const c_void);

                    //Normal map
                    gl::BindTexture(gl::TEXTURE_2D, textures[1]);
                    glutil::apply_texture_parameters(gl::TEXTURE_2D, &simple_tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA8 as GLint, 1, 1, 0, gl::RGBA, gl::FLOAT, &[0.5f32, 0.5, 1.0, 0.0] as *const f32 as *const c_void);

                    //Roughness map
                    gl::BindTexture(gl::TEXTURE_2D, textures[2]);
                    glutil::apply_texture_parameters(gl::TEXTURE_2D, &simple_tex_params);
                    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::R8 as GLint, 1, 1, 0, gl::RED, gl::FLOAT, &[0.5f32] as *const f32 as *const c_void);
                    using_cached_textures = false;
                }

                let transform_buffer = glutil::create_instanced_transform_buffer(vao.vao, instances, instanced_attribute);
                RenderEntity {
                    vao,
                    instanced_buffers: [0, 0, transform_buffer],
                    index_count: meshdata.vertex_array.indices.len() as GLint,
                    active_instances: instances,
                    shader: program,                    
                    material_textures: textures,
                    using_cached_textures,
                    lookup_texture: 0,
                    uv_velocity: glm::vec2(meshdata.uv_velocity[0], meshdata.uv_velocity[1]),
                    uv_scale: glm::vec2(1.0, 1.0),
                    uv_offset: glm::vec2(0.0, 0.0),
                    transparent: meshdata.is_transparent,
                    ignore_depth: false
                }
            }
            None => {
                tfd::message_box_ok("Error loading OzyMesh", &format!("Unable to load {}", path), MessageBoxIcon::Error);
                panic!("Unable to load OzyMesh: {}", path);
            }
        }
    }

    pub unsafe fn init_new_instanced_buffer(&mut self, floats_per: usize, attribute: GLuint, attribute_buffer_idx: usize) {
        gl::BindVertexArray(self.vao.vao);

        let data = vec![0.0f32; self.active_instances * floats_per];
        let mut b = 0;
        gl::GenBuffers(1, &mut b);
        gl::BindBuffer(gl::ARRAY_BUFFER, b);
        gl::BufferData(gl::ARRAY_BUFFER, (self.active_instances * floats_per * size_of::<GLfloat>()) as GLsizeiptr, &data[0] as *const f32 as *const c_void, gl::DYNAMIC_DRAW);
        self.instanced_buffers[attribute_buffer_idx] = b;
    
        gl::VertexAttribPointer(
            attribute,
            floats_per as GLint,
            gl::FLOAT,
            gl::FALSE,
            (floats_per * size_of::<GLfloat>()) as GLsizei,
            ptr::null()
        );
        gl::EnableVertexAttribArray(attribute);
        gl::VertexAttribDivisor(attribute, 1);
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
        let mut current_buffer_size = 0;
        gl::BindBuffer(gl::ARRAY_BUFFER, self.instanced_buffers[buffer_name_index]);
        gl::GetBufferParameteriv(gl::ARRAY_BUFFER, gl::BUFFER_SIZE, &mut current_buffer_size);

        if buffer.len() * size_of::<GLfloat>() > current_buffer_size as usize {
            let mut b = 0;
            gl::DeleteBuffers(1, &self.instanced_buffers[buffer_name_index] as *const u32);
            gl::GenBuffers(1, &mut b);
            self.instanced_buffers[buffer_name_index] = b;
            
            gl::BindBuffer(gl::ARRAY_BUFFER, self.instanced_buffers[buffer_name_index]);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.active_instances as usize * attribute_floats * size_of::<GLfloat>()) as GLsizeiptr,
                &buffer[0] as *const GLfloat as *const c_void,
                gl::DYNAMIC_DRAW
            );

            //Bad branch bad
            gl::BindVertexArray(self.vao.vao);
            if attribute_floats == 16 {
                glutil::bind_new_transform_buffer(attribute);
            } else {                
                gl::VertexAttribPointer(
                    attribute,
                    attribute_floats as GLint,
                    gl::FLOAT,
                    gl::FALSE,
                    (attribute_floats * size_of::<GLfloat>()) as GLsizei,
                    ptr::null()
                );
                gl::EnableVertexAttribArray(attribute);
                gl::VertexAttribDivisor(attribute, 1);
            }
        } else if buffer.len() > 0 {
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
        let new_instances = transforms.len() / 16;
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(transforms, instanced_attribute, 16, Self::TRANSFORM_BUFFER_INDEX); }
    }

    pub fn update_color_buffer(&mut self, colors: &[f32], instanced_attribute: GLuint) {
        //Record the current active instance count
        let new_instances = colors.len() / 4;
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(colors, instanced_attribute, 4, Self::COLOR_BUFFER_INDEX); }
    }

    pub fn update_highlight_buffer(&mut self, bools: &[f32], instanced_attribute: GLuint) {
        //Record the current active instance count
        let new_instances = bools.len();
        self.active_instances = new_instances;

        //Update GPU buffer storing transforms
        unsafe { self.write_buffer_to_GPU(bools, instanced_attribute, 1, Self::HIGHLIGHTED_BUFFER_INDEX); }
    }
}

impl Drop for RenderEntity {
    fn drop(&mut self) {
        let texs = if self.using_cached_textures {
            vec![self.lookup_texture]
        } else {
            vec![self.material_textures[0], self.material_textures[1], self.material_textures[2], self.lookup_texture]
        };

        unsafe {
            let bufs = [self.vao.vbo, self.vao.ebo, self.instanced_buffers[0], self.instanced_buffers[1], self.instanced_buffers[2]];
            gl::DeleteBuffers(bufs.len() as GLsizei, &bufs[0]);
            gl::DeleteVertexArrays(1, &self.vao.vao);
            gl::DeleteTextures(texs.len() as GLsizei, &texs[0]);
        }
    }
}

pub struct CascadedShadowMap {
    pub rendertarget: RenderTarget,             //Rendertarget for the shadow atlas
    pub program: GLuint,                        //Associated program name
    pub resolution: GLint,                      //The individual cascades are always squares
    pub matrices: [glm::TMat4<f32>; SHADOW_CASCADE_COUNT],
    pub clip_space_distances: [f32; SHADOW_CASCADE_COUNT + 1],
    pub view_space_distances: [f32; SHADOW_CASCADE_COUNT + 1]
}

impl CascadedShadowMap {
    pub fn new(rendertarget: RenderTarget, program: GLuint, resolution: GLint) -> Self {
        CascadedShadowMap {
            rendertarget,
            program,
            resolution,
            matrices: [glm::identity(); SHADOW_CASCADE_COUNT],
            clip_space_distances: [0.0; SHADOW_CASCADE_COUNT + 1],
            view_space_distances: [0.0; SHADOW_CASCADE_COUNT + 1]
        }
    }
}

pub struct PointLight {
    pub position: glm::TVec3<f32>,
    pub color: [f32; 3],
    pub power: f32,
    pub flicker_timescale: f32,
    pub flicker_amplitude: f32
}

impl PointLight {
    pub const COLLISION_RADIUS: f32 = 0.2;

    pub fn new(position: glm::TVec3<f32>, color: [f32; 3], power: f32) -> Self {
        PointLight {
            position,
            color,
            power,
            flicker_timescale: 1.0,
            flicker_amplitude: 0.0
        }
    }
}

impl SphereCollider for PointLight {
    fn sphere(&self) -> Sphere {
        Sphere {
            focus: self.position,
            radius: Self::COLLISION_RADIUS
        }
    }
}

pub struct SpotLight {
    pub point: PointLight,
    pub direction: glm::TVec3<f32>,
    pub angle: f32 //Expressed as cos(angle)
}

impl SphereCollider for SpotLight {
    fn sphere(&self) -> Sphere { self.point.sphere() }
}

pub struct SceneData {
    pub fragment_flag: FragmentFlag,
    pub complex_normals: bool,
    pub toon_shading: bool,
    pub using_postfx: bool,
    pub skybox_cubemap: GLuint,
    pub skybox_vao: GLuint,
    pub skybox_program: GLuint,
    pub depth_program: GLuint,
    pub ubo: GLuint,
    pub shininess_lower_bound: f32,
    pub shininess_upper_bound: f32,
    pub sun_pitch: f32,
    pub sun_yaw: f32,
    pub sun_direction: glm::TVec3<f32>,
    pub sun_color: [f32; 3],
    pub sun_size: f32,
    pub sun_shadow_map: CascadedShadowMap,
    pub shadow_intensity: f32,
    pub ambient_strength: f32,
    pub elapsed_time: f32,
    pub point_lights: OptionVec<PointLight>,
    pub point_lights_ubo: GLuint,
    pub selected_point_light: Option<usize>,
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
            view_space_distances: [0.0; SHADOW_CASCADE_COUNT + 1],
        };

        let ubo = {

            0
        };

        SceneData {
            fragment_flag: FragmentFlag::Default,
            complex_normals: true,
            toon_shading: true,
            using_postfx: false,
            skybox_cubemap: 0,
            skybox_vao: ozy::prims::skybox_cube_vao().vao,
            skybox_program: 0,
            depth_program: 0,
            ubo,
            shininess_lower_bound: 8.0,
            shininess_upper_bound: 128.0,
            sun_pitch: 0.0,
            sun_yaw: 0.0,
            sun_direction: glm::vec3(0.0, 0.0, 1.0),
            sun_color: [1.0, 1.0, 1.0],
            sun_size: 0.999,
            shadow_intensity: 1.0,
            ambient_strength: 0.2,
            sun_shadow_map,
            elapsed_time: 0.0,
            point_lights: OptionVec::new(),
            point_lights_ubo: 0,
            selected_point_light: None,
            opaque_entities: OptionVec::new(),
            transparent_entities: OptionVec::new()
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum FragmentFlag {
    Default,
    Albedo,
    Normals,
    CascadeZones,
    Shadowed
}
const FRAGMENT_FLAG_NAMES: [&str; 5] = ["visualize_albedo", "visualize_normals", "visualize_lod", "visualize_shadowed", "visualize_cascade_zone"];

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

//This is the function that renders the 3D scene
pub unsafe fn main_scene(framebuffer: &Framebuffer, scene_data: &SceneData, view_data: &ViewData) {
    //Main scene rendering
    gl::DepthMask(gl::TRUE);    //Enable depth writing
    framebuffer.bind();

    //Bind the shadow atlas
    gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
    gl::BindTexture(gl::TEXTURE_2D, scene_data.sun_shadow_map.rendertarget.texture);

    //Bind the skybox sampler
    gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum + 1);
    gl::BindTexture(gl::TEXTURE_CUBE_MAP, scene_data.skybox_cubemap);

    //Depth pre-pass
    for opt_entity in scene_data.opaque_entities.iter() {
        if let Some(entity) = &opt_entity {
            render_entity(entity, scene_data.depth_program, scene_data, view_data);
        }
    }

    gl::DepthMask(gl::FALSE);               //Disable depth writing
    
    //Render opaque geometry
    for opt_entity in scene_data.opaque_entities.iter() {
        if let Some(entity) = &opt_entity {
            render_entity(entity, entity.shader, scene_data, view_data);
        }
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
    glutil::bind_vector3(scene_data.skybox_program, "sun_direction", &scene_data.sun_direction);
    glutil::bind_float(scene_data.skybox_program, "sun_size", scene_data.sun_size);
    gl::BindTexture(gl::TEXTURE_CUBE_MAP, scene_data.skybox_cubemap);
    gl::BindVertexArray(scene_data.skybox_vao);
    gl::DrawElements(gl::TRIANGLES, CUBE_INDICES_COUNT, gl::UNSIGNED_SHORT, ptr::null());

    //Render transparent geometry
    for opt_entity in scene_data.transparent_entities.iter() {
        if let Some(entity) = &opt_entity {
            render_entity(entity, entity.shader, scene_data, view_data);
        }
    }
}

unsafe fn render_entity(entity: &RenderEntity, program: GLuint, scene_data: &SceneData, view_data: &ViewData) {
    
    let texture_sampler_names = ["albedo_sampler", "normal_sampler", "roughness_sampler", "shadow_map"];

    let p = program;
    gl::UseProgram(p);

    //TODO: Use a uniform buffer object to avoid binding these for each entity
    let sun_c = glm::vec3(scene_data.sun_color[0], scene_data.sun_color[1], scene_data.sun_color[2]);
    glutil::bind_vector3(p, "sun_color", &sun_c);
    glutil::bind_matrix4_array(p, "shadow_matrices", &scene_data.sun_shadow_map.matrices);
    glutil::bind_float(p, "shininess_lower_bound", scene_data.shininess_lower_bound);
    glutil::bind_float(p, "shininess_upper_bound", scene_data.shininess_upper_bound);
    glutil::bind_vector3(p, "sun_direction", &scene_data.sun_direction);
    glutil::bind_float(p, "ambient_strength", scene_data.ambient_strength);
    glutil::bind_float(p, "shadow_intensity", scene_data.shadow_intensity);
    glutil::bind_float(p, "current_time", scene_data.elapsed_time);
    glutil::bind_int(p, "complex_normals", scene_data.complex_normals as GLint);
    glutil::bind_int(p, "toon_shading", scene_data.toon_shading as GLint);
    glutil::bind_int(p, "point_lights_count", scene_data.point_lights.count() as GLint);
    glutil::bind_float_array(p, "cascade_distances", &scene_data.sun_shadow_map.clip_space_distances[1..]);
    glutil::bind_matrix4(p, "view_projection", &view_data.view_projection);
    glutil::bind_int(p, "shadow_map", ENTITY_TEXTURE_COUNT as GLint);
    glutil::bind_int(p, "skybox_sampler", ENTITY_TEXTURE_COUNT as GLint + 1);
    glutil::bind_vector3(p, "view_position", &view_data.view_position);

    //fragment flag stuff        
    for name in FRAGMENT_FLAG_NAMES.iter() {
        glutil::bind_int(p, name, 0);
    }
    match scene_data.fragment_flag {
        FragmentFlag::Albedo => { glutil::bind_int(p, "visualize_albedo", 1); }
        FragmentFlag::Shadowed => { glutil::bind_int(p, "visualize_shadowed", 1); }
        FragmentFlag::Normals => { glutil::bind_int(p, "visualize_normals", 1); }
        FragmentFlag::CascadeZones => { glutil::bind_int(p, "visualize_cascade_zone", 1); }
        FragmentFlag::Default => {}
    }    

    //These actually do need to be bound per entity
    glutil::bind_vector2(p, "uv_velocity", &entity.uv_velocity);
    glutil::bind_vector2(p, "uv_scale", &entity.uv_scale);
    glutil::bind_vector2(p, "uv_offset", &entity.uv_offset);
    for i in 0..ENTITY_TEXTURE_COUNT {
        glutil::bind_int(p, texture_sampler_names[i], i as GLint);
        gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
        gl::BindTexture(gl::TEXTURE_2D, entity.material_textures[i]);
    }

    //Bind lookup texture
    gl::ActiveTexture(gl::TEXTURE0);
    gl::BindTexture(gl::TEXTURE_1D, entity.lookup_texture);
    
    if entity.ignore_depth {
        gl::Disable(gl::DEPTH_TEST);
    }

    gl::BindVertexArray(entity.vao.vao);
    gl::DrawElementsInstanced(gl::TRIANGLES, entity.index_count, gl::UNSIGNED_SHORT, ptr::null(), entity.active_instances as GLint);
    
    if entity.ignore_depth {
        gl::Enable(gl::DEPTH_TEST);
    }   
}

pub unsafe fn cascaded_shadow_map(shadow_map: &CascadedShadowMap, entities: &[Option<RenderEntity>]) {    
    gl::DepthMask(gl::TRUE);
    shadow_map.rendertarget.framebuffer.bind();
    gl::UseProgram(shadow_map.program);
    for i in 0..SHADOW_CASCADE_COUNT {
        //Configure rendering for this cascade
        glutil::bind_matrix4(shadow_map.program, "view_projection", &shadow_map.matrices[i]);
        gl::Viewport(i as GLint * shadow_map.resolution, 0, shadow_map.resolution, shadow_map.resolution);

        for opt_entity in entities.iter() {
            if let Some(entity) = opt_entity {
                gl::BindVertexArray(entity.vao.vao);
                gl::DrawElementsInstanced(gl::TRIANGLES, entity.index_count, gl::UNSIGNED_SHORT, ptr::null(), entity.active_instances as GLint);
            }
        }
    }
}

pub fn compute_shadow_cascade_matrices(
    shadow_cascade_distances: &[f32; SHADOW_CASCADE_COUNT + 1],
    shadow_view: &glm::TMat4<f32>,
    v_mat: &glm::TMat4<f32>,
    projection: &glm::TMat4<f32>
) -> [glm::TMat4<f32>; SHADOW_CASCADE_COUNT] {
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

pub unsafe fn post_processing(fbo_texture_view: GLuint, window_size: glm::TVec2<u32>, postfx_program: GLuint, elapsed_time: f32) {
    gl::BindTexture(gl::TEXTURE_2D, fbo_texture_view);
    gl::GenerateMipmap(gl::TEXTURE_2D);

    //Binding the compute shader program
    gl::UseProgram(postfx_program);
    glutil::bind_float(postfx_program, "elapsed_time", elapsed_time);

    //ping_rt.color_attachment_view is a texture view into ping_rt.texture but it's internal format is set to gl::RGBA8 so the compute shader can use it
    gl::BindImageTexture(0, fbo_texture_view, 0, gl::FALSE, 0, gl::READ_WRITE, gl::RGBA8);

    //Dispatching compute. Extra groups may be required if the resolution isn't divisible by 32x32
    let kernel_size = 32;
    let x_groups = (window_size.x + kernel_size - 1) / kernel_size;
    let y_groups = (window_size.y + kernel_size - 1) / kernel_size;
    gl::DispatchCompute(x_groups, y_groups, 1);

    //Waiting for the compute shader to finish before blitting to the default framebuffer
    gl::MemoryBarrier(gl::FRAMEBUFFER_BARRIER_BIT);
}

pub unsafe fn blit_full_color_buffer(src_fb: &Framebuffer, dst_fb: &Framebuffer) {
    gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, dst_fb.name);
    gl::BindFramebuffer(gl::READ_FRAMEBUFFER, src_fb.name);
    gl::BlitFramebuffer(
        0,
        0,
        src_fb.size.0 as GLint,
        src_fb.size.1 as GLint,
        0,
        0,
        dst_fb.size.0 as GLint,
        dst_fb.size.1 as GLint,
        gl::COLOR_BUFFER_BIT,
        gl::NEAREST
    );
}