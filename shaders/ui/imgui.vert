#version 430 core

in vec2 position;
in vec2 uv;
in vec4 color;

out vec2 f_uv;
out vec4 f_color;

uniform mat4 projection;

void main() {
    f_uv = uv;
    f_color = color;
    gl_Position = projection * vec4(position, 0.0, 1.0);
}