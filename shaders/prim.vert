#version 330 core

in vec3 position;
in vec3 normal;

out vec4 f_normal;

uniform mat4 mvp;

void main() {
    f_normal = vec4(normal, 0.0);
    gl_Position = mvp * vec4(position, 1.0);
}