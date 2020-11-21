#version 330 core

layout (location = 0) in vec3 position;
layout (location = 1) in vec3 tangent;
layout (location = 2) in vec3 bitangent;
layout (location = 3) in vec3 normal;
layout (location = 4) in vec2 uv;

layout (location = 5) in mat4 model_matrix;

uniform mat4 view_projection;

void main() {
    gl_Position = view_projection * model_matrix * vec4(position, 1.0);
}