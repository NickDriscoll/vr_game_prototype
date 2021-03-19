#version 430 core

layout (location = 0) in vec3 position;
layout (location = 5) in mat4 model_matrix;

uniform mat4 view_projection;

void main() {
    gl_Position = view_projection * model_matrix * vec4(position, 1.0);
}