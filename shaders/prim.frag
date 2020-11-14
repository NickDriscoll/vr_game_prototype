#version 330 core

in vec4 f_normal;

out vec4 frag_color;

uniform vec4 color;

void main() {
    frag_color = color;
}