#version 330 core

layout (location = 0) in vec3 position;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 uv;

out vec3 f_normal;
out vec4 world_space_pos;
out vec4 shadow_space_pos;
out vec2 f_uvs;

uniform mat4 mvp;
uniform mat4 model_matrix;
uniform mat4 shadow_matrix;

void main() {
    mat3 normal_matrix = transpose(inverse(mat3(model_matrix)));
    f_normal = normal_matrix * normal;

    world_space_pos = model_matrix * vec4(position, 1.0);
    shadow_space_pos = shadow_matrix * world_space_pos;

    f_uvs = uv;
    gl_Position = mvp * vec4(position, 1.0);
}