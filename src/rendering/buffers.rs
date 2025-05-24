use eframe::glow::{self, NativeBuffer, NativeVertexArray};
use eframe::glow::HasContext;
use std::sync::Arc;

pub struct Buffer {
    pub buffer: NativeBuffer,
    target: u32,
    gl: Arc<glow::Context>
}

impl Buffer {
    pub fn new(gl: Arc<glow::Context>, target: u32) -> Self {
        unsafe {
            let buffer = gl.create_buffer().unwrap();
            Self {
                buffer,
                target,
                gl
            }
        }
    }

    pub fn bind(&self) {
        unsafe {
            self.gl.bind_buffer(self.target, Some(self.buffer));
        }
    }

    pub fn set_data<D>(&self, data: &[D], usage: u32) {
        unsafe { 
            self.bind();
            let (_, data_bytes, _) = data.align_to::<u8>();
            self.gl.buffer_data_u8_slice(
                self.target, 
                data_bytes, 
                usage
            );
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_buffer(self.buffer);
        }
    }
}

pub struct VertexArray {
    pub array: NativeVertexArray,
    gl: Arc<glow::Context>
}

impl VertexArray {
    pub fn new(gl: Arc<glow::Context>) -> Self {
        unsafe {
            Self {
                array: gl.create_vertex_array().unwrap(),
                gl
            }
        }
    }

    pub fn bind(&self) {
        unsafe {
            self.gl.bind_vertex_array(Some(self.array));
        }
    }

    pub fn set_attribute<V: Sized>(
        &self,
        type_: u32,
        attrib_pos: u32,
        components: i32,
        offset: i32
    ) {
        unsafe {
            self.bind();
            if type_ == glow::FLOAT {
                self.gl.vertex_attrib_pointer_f32(
                    attrib_pos,
                    components,
                    type_,
                    false, 
                    std::mem::size_of::<V>() as i32,
                    offset
                );
            } else {
                self.gl.vertex_attrib_pointer_i32(
                    attrib_pos,
                    components,
                    type_,
                    std::mem::size_of::<V>() as i32,
                    offset
                );
            }

            self.gl.enable_vertex_attrib_array(attrib_pos);
        }
    }
}

impl Drop for VertexArray {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_vertex_array(self.array);
        }
    }
}

#[macro_export]
macro_rules! set_attribute {
    ($type_:ident :: $tfield:tt, $vbo:ident, $pos:tt, $t:ident :: $field:tt) => {{
        let dummy = core::mem::MaybeUninit::<$t>::uninit();
        let dummy_ptr = dummy.as_ptr();
        let member_ptr = core::ptr::addr_of!((*dummy_ptr).$field);
        const fn size_of_raw<T>(_: *const T) -> usize {
            core::mem::size_of::<T>()
        }
        let member_offset = member_ptr as i32 - dummy_ptr as i32;
        $vbo.set_attribute::<$t>(
            $type_::$tfield,
            $pos,
            (size_of_raw(member_ptr) / 4) as i32,
            member_offset,
        )
    }};
}