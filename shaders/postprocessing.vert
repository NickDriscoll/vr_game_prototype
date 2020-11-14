#version 330 core

in vec2 position;
in vec2 uvs;

out vec2 f_uvs;

void main() {
    f_uvs = uvs;
    gl_Position = vec4(position, 0.0, 1.0);
}