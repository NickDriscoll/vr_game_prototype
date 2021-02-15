use std::ptr;
use ozy::render::{InstancedMesh, RenderTarget, SimpleMesh};
use crate::glutil;
use gl::types::*;

pub const NEAR_DISTANCE: f32 = 0.0625;
pub const FAR_DISTANCE: f32 = 100000.0;

pub struct SingleEntity {
    pub mesh: SimpleMesh,
    pub visible: bool,
    pub uv_scale: glm::TVec2<f32>,
    pub uv_offset: glm::TVec2<f32>,
    pub model_matrix: glm::TMat4<f32>
}

pub struct InstancedEntity {
    pub mesh: InstancedMesh,
    pub visible: bool,
    pub uv_offset: glm::TVec2<f32>,
    pub uv_scale: glm::TVec2<f32>
}

pub struct SceneData {
    pub visualize_normals: bool,
    pub complex_normals: bool,
    pub outlining: bool,
    pub shadow_texture: GLuint,
    pub skybox_cubemap: GLuint,
    pub skybox_vao: GLuint,
    pub uniform_light: glm::TVec4<f32>,
    pub shadow_matrix: glm::TMat4<f32>,
    pub programs: [GLuint; 5],              //non-instanced , instanced  , skybox , single-shadow , instanced-shadow
    pub single_entities: Vec<SingleEntity>,
    pub instanced_entities: Vec<InstancedEntity>,
}

impl SceneData {
    const SINGULAR_PROGRAM_INDEX: usize = 0;
    const INSTANCED_PROGRAM_INDEX: usize = 1;
    const SKYBOX_PROGRAM_INDEX: usize = 2;
    const SINGLE_SHADOW_PROGRAM_INDEX: usize = 3;
    const INSTANCED_SHADOW_PROGRAM_INDEX: usize = 4;

    //Returns the entity's index
    pub fn push_single_entity(&mut self, mesh: SimpleMesh) -> usize {
        let entity = SingleEntity {
            visible: true,
            mesh: mesh,
            uv_scale: glm::vec2(1.0, 1.0),
            uv_offset: glm::zero(),
            model_matrix: glm::identity()
        };
        self.single_entities.push(entity);
        self.single_entities.len() - 1
    }

    //Returns the entity's index
    pub fn push_instanced_entity(&mut self, mesh: InstancedMesh) -> usize {
        let entity = InstancedEntity {
            visible: true,
            mesh: mesh,
            uv_offset: glm::zero(),
            uv_scale: glm::vec2(1.0, 1.0)
        };
        self.instanced_entities.push(entity);
        self.instanced_entities.len() - 1
    }
}

impl Default for SceneData {
    fn default() -> Self {
        SceneData {
            visualize_normals: false,
            complex_normals: true,
            outlining: false,
            shadow_texture: 0,
            skybox_cubemap: 0,
            skybox_vao: 0,
            uniform_light: glm::vec4(0.0, 0.0, 1.0, 0.0),
            shadow_matrix: glm::identity(),
            programs: [0, 0, 0, 0, 0],
            single_entities: Vec::new(),
            instanced_entities: Vec::new()
        }
    }
}

pub struct ViewData {
    pub view_position: glm::TVec4<f32>,
    pub view_matrix: glm::TMat4<f32>,
    pub projection_matrix: glm::TMat4<f32>,
    pub view_projection: glm::TMat4<f32>
}

impl ViewData {
    pub fn new(view_position: glm::TVec4<f32>, view_matrix: glm::TMat4<f32>, projection_matrix: glm::TMat4<f32>) -> Self {
        Self {
            view_position,
            view_matrix,
            projection_matrix,
            view_projection: projection_matrix * view_matrix
        }
    }
}

pub unsafe fn render_main_scene(scene_data: &SceneData, view_data: &ViewData) {

    let texture_map_names = ["albedo_map", "normal_map", "roughness_map", "shadow_map"];

    //Main scene rendering
    //framebuffer.bind();
    gl::ActiveTexture(gl::TEXTURE0 + ozy::render::TEXTURE_MAP_COUNT as GLenum);
    gl::BindTexture(gl::TEXTURE_2D, scene_data.shadow_texture);
                        
    //Bind common uniforms
    for program in &scene_data.programs {
        glutil::bind_matrix4(*program, "shadow_matrix", &scene_data.shadow_matrix);
        glutil::bind_matrix4(*program, "view_projection", &view_data.view_projection);
        glutil::bind_vector4(*program, "sun_direction", &scene_data.uniform_light);
        glutil::bind_int(*program, "shadow_map", ozy::render::TEXTURE_MAP_COUNT as GLint);
        glutil::bind_int(*program, "visualize_normals", scene_data.visualize_normals as GLint);
        glutil::bind_int(*program, "complex_normals", scene_data.complex_normals as GLint);
        glutil::bind_int(*program, "outlining", scene_data.outlining as GLint);
        glutil::bind_vector4(*program, "view_position", &view_data.view_position);

        for i in 0..ozy::render::TEXTURE_MAP_COUNT {
            glutil::bind_int(*program, texture_map_names[i], i as GLint);
        }
    }

    //Render non-instanced entities
    gl::UseProgram(scene_data.programs[SceneData::SINGULAR_PROGRAM_INDEX]);
    for entity in scene_data.single_entities.iter() {
        if entity.visible {
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, entity.mesh.texture_maps[i]);
            }        
            glutil::bind_matrix4(scene_data.programs[SceneData::SINGULAR_PROGRAM_INDEX], "model_matrix", &entity.model_matrix);
            glutil::bind_vector2(scene_data.programs[SceneData::SINGULAR_PROGRAM_INDEX], "uv_scale", &entity.uv_scale);
            glutil::bind_vector2(scene_data.programs[SceneData::SINGULAR_PROGRAM_INDEX], "uv_offset", &entity.uv_offset);
            entity.mesh.draw();
        }
    }

    //Instanced entity rendering
    gl::UseProgram(scene_data.programs[SceneData::INSTANCED_PROGRAM_INDEX]);
    for entity in scene_data.instanced_entities.iter() {
        if entity.visible {
            for i in 0..ozy::render::TEXTURE_MAP_COUNT {
                gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
                gl::BindTexture(gl::TEXTURE_2D, entity.mesh.texture_maps()[i]);
            }
            glutil::bind_vector2(scene_data.programs[SceneData::INSTANCED_PROGRAM_INDEX], "uv_offset", &entity.uv_offset);
            glutil::bind_vector2(scene_data.programs[SceneData::INSTANCED_PROGRAM_INDEX], "uv_scale", &entity.uv_scale);
            entity.mesh.draw();
        }
    }

    //Skybox rendering
    
	//Compute the view-projection matrix for the skybox (the conversion functions are just there to nullify the translation component of the view matrix)
	//The skybox vertices should obviously be rotated along with the camera, but they shouldn't be translated in order to maintain the illusion
	//that the sky is infinitely far away
    let skybox_view_projection = view_data.projection_matrix * glm::mat3_to_mat4(&glm::mat4_to_mat3(&view_data.view_matrix));

    //Render the skybox
    gl::UseProgram(scene_data.programs[SceneData::SKYBOX_PROGRAM_INDEX]);
    glutil::bind_matrix4(scene_data.programs[SceneData::SKYBOX_PROGRAM_INDEX], "view_projection", &skybox_view_projection);
    gl::BindTexture(gl::TEXTURE_CUBE_MAP, scene_data.skybox_cubemap);
    gl::BindVertexArray(scene_data.skybox_vao);
    gl::DrawElements(gl::TRIANGLES, 36, gl::UNSIGNED_SHORT, ptr::null());
}

pub unsafe fn render_shadows(scene_data: &SceneData) {
    //Draw instanced meshes into shadow map
    glutil::bind_matrix4(scene_data.programs[SceneData::INSTANCED_SHADOW_PROGRAM_INDEX], "view_projection", &scene_data.shadow_matrix);
    gl::UseProgram(scene_data.programs[SceneData::INSTANCED_SHADOW_PROGRAM_INDEX]);
    for entity in &scene_data.instanced_entities {
        if entity.visible {
            entity.mesh.draw();
        }
    }

    //Draw simple meshes into shadow map
    gl::UseProgram(scene_data.programs[SceneData::SINGLE_SHADOW_PROGRAM_INDEX]);
    for entity in &scene_data.single_entities {
        if entity.visible {
            glutil::bind_matrix4(scene_data.programs[SceneData::SINGLE_SHADOW_PROGRAM_INDEX], "mvp", &(scene_data.shadow_matrix * entity.model_matrix));
            entity.mesh.draw();
        }
    }
}