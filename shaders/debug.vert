#version 430 core

layout (location = 0) in vec3 position;
layout (location = 1) in vec4 color;
layout (location = 2) in mat4 model_matrix;

out vec4 f_color;

uniform mat4 view_projection;

void main() {
    f_color = color;
    gl_Position = view_projection * model_matrix * vec4(position, 1.0);
}