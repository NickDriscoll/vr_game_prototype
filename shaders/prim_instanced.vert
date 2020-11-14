#version 330 core

layout (location = 0) in vec3 position;
layout (location = 1) in vec3 normal;
layout (location = 2) in mat4 model_matrix;

out vec4 f_normal;

uniform mat4 view_projection;

void main() {
    f_normal = vec4(normal, 0.0);
    gl_Position = view_projection * model_matrix * vec4(position, 1.0);
}