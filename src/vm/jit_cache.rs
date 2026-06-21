use std::collections::HashMap;
use crate::tinyasm::jit::JitMemory;
use crate::vm::jit_abi::JitFrame;
use crate::vm::bytecode::Chunk;

struct JitEntry {
    exec_count: u32,
    compiled: Option<(JitMemory, unsafe extern "C" fn(*mut JitFrame))>,
}

pub struct JitCache {
    entries: HashMap<String, JitEntry>,
}

impl JitCache {
    const HOT_THRESHOLD: u32 = 10;

    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn tick(&mut self, key: &str, chunk: &Chunk) -> Option<unsafe extern "C" fn(*mut JitFrame)> {
        let entry = self.entries.entry(key.to_string()).or_insert(JitEntry {
            exec_count: 0,
            compiled: None,
        });

        if let Some((_, fp)) = &entry.compiled {
            return Some(*fp);
        }

        entry.exec_count += 1;
        if entry.exec_count >= Self::HOT_THRESHOLD {
            if let Ok((mem, fp)) = crate::vm::jit_compiler::JitCompiler::compile_chunk(chunk) {
                entry.compiled = Some((mem, fp));
                return Some(fp);
            }
        }
        
        None
    }
}
