#version 430 core

//Vertex data
layout (location = 0) in vec3 position;
layout (location = 1) in vec3 tangent;
layout (location = 2) in vec3 bitangent;
layout (location = 3) in vec3 normal;
layout (location = 4) in vec2 uv;

//Instanced arrays
layout (location = 5) in float highlighted;
layout (location = 6) in mat4 model_matrix;

struct PointLight {
    vec3 position;
    vec3 color;
    float radius;
};

const int SHADOW_CASCADES = 6;

out vec3 tangent_sun_direction;
out vec3 tangent_view_position;
out vec3 tangent_space_pos;
out vec4 shadow_space_pos[SHADOW_CASCADES];
out vec3 f_world_pos;
out vec2 scaled_uvs;
out float clip_space_z;
out float f_highlighted;

uniform mat4 view_projection;
uniform mat4 shadow_matrices[SHADOW_CASCADES];
uniform vec3 sun_direction;
uniform vec3 view_position;
uniform vec2 uv_scale = vec2(1.0, 1.0);
uniform vec2 uv_offset = vec2(0.0, 0.0);


void main() {
    mat4 normal_matrix = transpose(mat4(inverse(mat3(model_matrix))));
    vec3 T = normalize(vec3(normal_matrix * vec4(tangent, 0.0)));
    vec3 B = normalize(vec3(normal_matrix * vec4(bitangent, 0.0)));
    vec3 N = normalize(vec3(normal_matrix * vec4(normal, 0.0)));
    mat3 tangent_from_world = transpose(mat3(T, B, N));

    vec4 world_space_pos = model_matrix * vec4(position, 1.0);
    for (int i = 0; i < SHADOW_CASCADES; i++) {
        shadow_space_pos[i] = shadow_matrices[i] * world_space_pos;
    }    

    tangent_space_pos = tangent_from_world * vec3(world_space_pos);
    tangent_sun_direction = tangent_from_world * sun_direction;
    tangent_view_position = tangent_from_world * view_position;
    f_world_pos = vec3(world_space_pos);
    
    scaled_uvs = uv * uv_scale + uv_offset;
    
    f_highlighted = highlighted;

    gl_Position = view_projection * world_space_pos;

    clip_space_z = gl_Position.z;
}