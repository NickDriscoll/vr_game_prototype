#version 330 core

in vec2 position;
in vec4 color;

out vec4 f_color;

uniform mat4 clipping_from_screen;

void main() {
    f_color = color;
    gl_Position = clipping_from_screen * vec4(position, 0.0, 1.0);
}