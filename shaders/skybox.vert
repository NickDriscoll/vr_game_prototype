#version 330 core
in vec3 position;

out vec3 tex_coord;

uniform mat4 view_projection;

void main() {
    //Rotate the uv coords so that they're correct for right-handed z-up
    tex_coord = vec3(position.x, position.z, -position.y);
    vec4 screen_space_pos = view_projection * vec4(position, 1.0);
    gl_Position = screen_space_pos.xyww;                                //Set z = w so that z/w == 1.0
}