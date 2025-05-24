use std::fs::File;
use std::io::Read;
use std::path::absolute;

use eframe::glow::NativeProgram;
use eframe::{glow};
use eframe::glow::HasContext;

use std::sync::Arc;

pub struct ShaderProgram {
    pub program: NativeProgram,
    gl: Arc<glow::Context>
}

impl ShaderProgram {
    /// This will create a OpenGL Shader program directly from the path to the shaders.
    /// 
    /// The program itself can be accessed by [`program`]
    pub fn create_from_files(gl: Arc<glow::Context>, shader_path: &'static str) -> Self {
        let mut file_vert = File::open(
            absolute(format!("{}.vert", shader_path)).unwrap()
        ).unwrap();
        let mut src_vert = String::new();
        file_vert.read_to_string(&mut src_vert).unwrap();

        let mut file_frag = File::open(
            absolute(format!("{}.frag", shader_path)).unwrap()
        ).unwrap();
        let mut src_frag = String::new();
        file_frag.read_to_string(&mut src_frag).unwrap();

        unsafe {
            let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(vert, &src_vert);
            gl.compile_shader(vert);
            assert!(gl.get_shader_compile_status(vert), "Vertex shader error");

            let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(frag, &src_frag);
            gl.compile_shader(frag);
            assert!(gl.get_shader_compile_status(frag), "Fragment shader error");

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, vert);
            gl.attach_shader(program, frag);
            gl.link_program(program);
            assert!(gl.get_program_link_status(program), "Program link error");

            gl.delete_shader(vert);
            gl.delete_shader(frag);
            
            Self {
                program,
                gl
            }
        }
    }

    pub fn get_attrib_location(&self, attrib: &str) -> Option<u32> {
        unsafe { self.gl.get_attrib_location(self.program, attrib) }
    }

    pub fn set_float(&self, name: &str, value: f32) {
        unsafe {
            self.gl.uniform_1_f32(
                self.gl.get_uniform_location(self.program, name).as_ref(),
                value
            )
        }
    }
}