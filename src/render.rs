use std::collections::HashMap;
use std::ptr;
use ozy::render::{Framebuffer, InstancedMesh, SimpleMesh};
use crate::glutil;
use gl::types::*;

pub const NEAR_DISTANCE: f32 = 0.0625;
pub const FAR_DISTANCE: f32 = 100000.0;

pub struct SingleEntity {
    pub mesh: SimpleMesh,
    pub uv_scale: f32,
    pub model_matrix: glm::TMat4<f32>
}

pub struct InstancedEntity {
    pub mesh: InstancedMesh,
    pub uv_scale: f32
}

pub struct SceneData {
    pub visualize_normals: bool,
    pub complex_normals: bool,
    pub outlining: bool,
    pub shadow_texture: GLuint,
    pub uniform_light: glm::TVec4<f32>,
    pub shadow_matrix: glm::TMat4<f32>,
    pub programs: [GLuint; 2],              //non-instanced program followed by instanced program
    pub single_entities: Vec<SingleEntity>,
    pub instanced_entities: Vec<InstancedEntity>,
}

impl SceneData {
    //Returns the entity's index
    pub fn push_single_entity(&mut self, mesh: SimpleMesh) -> usize {
        let entity = SingleEntity {
            mesh: mesh,
            uv_scale: 1.0,
            model_matrix: glm::identity()
        };
        self.single_entities.push(entity);
        self.single_entities.len() - 1
    }

    //Returns the entity's index
    pub fn push_instanced_entity(&mut self, mesh: InstancedMesh) -> usize {
        let entity = InstancedEntity {
            mesh: mesh,
            uv_scale: 1.0
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
            uniform_light: glm::vec4(0.0, 0.0, 1.0, 0.0),
            shadow_matrix: glm::identity(),
            programs: [0, 0],
            single_entities: Vec::new(),
            instanced_entities: Vec::new()
        }
    }
}

pub struct ViewData {
    pub view_position: glm::TVec4<f32>,
    pub view_projection: glm::TMat4<f32>
}

pub unsafe fn render_main_scene(scene_data: &SceneData, view_data: &ViewData) {
    const SINGULAR_PROGRAM_INDEX: usize = 0;
    const INSTANCED_PROGRAM_INDEX: usize = 1;
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
    gl::UseProgram(scene_data.programs[SINGULAR_PROGRAM_INDEX]);
    for entity in scene_data.single_entities.iter() {
        for i in 0..ozy::render::TEXTURE_MAP_COUNT {
            gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
            gl::BindTexture(gl::TEXTURE_2D, entity.mesh.texture_maps[i]);
        }        
        glutil::bind_matrix4(scene_data.programs[SINGULAR_PROGRAM_INDEX], "model_matrix", &entity.model_matrix);
        glutil::bind_float(scene_data.programs[SINGULAR_PROGRAM_INDEX], "uv_scale", entity.uv_scale);
        entity.mesh.draw();
    }

    //Instanced entity rendering
    gl::UseProgram(scene_data.programs[INSTANCED_PROGRAM_INDEX]);
    for entity in scene_data.instanced_entities.iter() {
        for i in 0..ozy::render::TEXTURE_MAP_COUNT {
            gl::ActiveTexture(gl::TEXTURE0 + i as GLenum);
            gl::BindTexture(gl::TEXTURE_2D, entity.mesh.texture_maps()[i]);
        }
        glutil::bind_float(scene_data.programs[INSTANCED_PROGRAM_INDEX], "uv_scale", entity.uv_scale);
        entity.mesh.draw();
    }
}