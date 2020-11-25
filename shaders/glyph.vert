#version 330 core

in vec2 position;
in vec2 uvs;
in vec4 color;

out vec2 f_uvs;
out vec4 f_color;

uniform mat4 clipping_from_screen;

void main() {
    f_uvs = uvs;
    f_color = color;
    gl_Position = clipping_from_screen * vec4(position, 0.0, 1.0);
}