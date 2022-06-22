#![allow(unused_must_use)]
#![allow(dead_code)]

use std::str;
use std::mem;
use std::env;
use std::ffi::OsStr;
use fuse::{Filesystem, Request, ReplyCreate, ReplyEmpty, ReplyAttr, ReplyEntry, ReplyOpen, ReplyData, ReplyDirectory, ReplyWrite, FileType, FileAttr};
use time::get_time;
use time::Timespec;
use libc::{getuid, getgid, ENOSYS, ENOENT, EIO, EISDIR, ENOSPC};
use serde::{Serialize, Deserialize};
use bincode::{serialize, deserialize};
use qrcode::QrCode;
use image::Luma;
use std::path::PathBuf;
use native_dialog::FileDialog;
extern crate ncurses;
use ncurses::{getch, initscr, addstr, endwin, refresh, clear};

const DEFAULT_SIZE:usize = 1024;
const MAX_FILES_DIRECTORY:usize = 64;
const NAMELEN: u32 = 64;

/*
    Estructura fundamental que nos sirve para administrar los archivos
*/
struct Disk {
    super_block: Vec<Option<Inode>>,
    memory_blocks: Vec<MemoryBlock>,
    max_files: usize,
    block_size: usize,
    root_path: String
}

impl Disk {
    /*
        Función que permite la creación de un disco
        E: el path raíz, tamaño de la memoria, tamaño del bloque
        S: un nuevo Disk
    */
    fn new(root_path: String, memory_size: usize, block_size: usize) -> Disk{
        let memory_quantity: usize = (memory_size / block_size) - 1;
        let inode_size = mem::size_of::<Vec<Inode>>() + mem::size_of::<Inode>();
        let max_files = block_size / inode_size;

        let mut memory_blocks: Vec<MemoryBlock>;
        let mut super_block: Vec<Option<Inode>>;

        super_block = Vec::new();
        memory_blocks = Vec::new();

        let ttl:Timespec = get_time();
        let attributes = FileAttr {
            ino: 1,
            size: 0,
            blocks: 0,
            atime: ttl,
            mtime: ttl,
            ctime: ttl,
            crtime: ttl,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 0,
            uid: unsafe{getuid()},
            gid: unsafe{getgid()},
            rdev: 0,
            flags: 0
        };

        let name:String = String::from(".");

        let root_inode = Inode {
            name,
            attributes,
            references: vec![Option::None; MAX_FILES_DIRECTORY]
        };

        super_block.push(Some(root_inode));

        //Inicializar los demás campos en NONE

        for _ in super_block.len()..max_files {
            super_block.push(Option::None);
        }

        for _ in memory_blocks.len()..memory_quantity {
            memory_blocks.push(MemoryBlock { data: Option::None });
        }

        println!("Disco Inicializado Correctamente");

        Disk { 
            super_block, 
            memory_blocks, 
            max_files, 
            block_size, 
            root_path 
        }

    }

    /*
        Función que encuentra el siguiente ino que esté vacío = None
        E: N/A
        S: un opcional del índice al siguiente ino
    */
    fn find_next_ino(&self) -> Option<u64>{
        for i in 0..self.super_block.len() - 1 {
            if let Option::None = self.super_block[i] {
                let ino = (i as u64) + 1;
                return Option::Some(ino);
            }
        }

        Option::None
    }

    /*
        Función que encuentra un memory block vacío
        E: N/A
        S: un opcional de índice al memory block vacío
    */
    fn find_empty_memory_block(&self) -> Option<usize> {
        for i in 0..self.memory_blocks.len() - 1 {
            if let Option::None = self.memory_blocks[i].data {
                return Option::Some(i);
            }
        }

        Option::None
    }

    /*
        Función que encuentra una referencia vacío dentro de un inode
        E: ino (identificador del inode)
        S: un opcional de índice al espacio vacío de la referencia
    */
    fn find_empty_reference(&self, ino:u64) -> Option<usize> {
        let position = (ino as usize) - 1;

        match &self.super_block[position] {
            Some(inode) => inode.references.iter().position(|i| i == &None),
            None => panic!("Espacio de memoria inválido")
        }
    }

    /*
        Función que guarda un nuevo inode en el super block
        E: inode que se desea guardar
        S: N/A
    */
    fn write_inode(&mut self, inode: Inode) {
        if mem::size_of_val(&inode) > self.block_size {
            println!("Error: Tamaño del Inode es incorrecto");
            return ;
        }

        let position = (inode.attributes.ino - 1) as usize;
        self.super_block[position] = Some(inode);
    }
    
    /*
        Función que elimina un inode del superblock
        E: ino (identificador del inode)
        S: N/A
    */
    fn remove_inode(&mut self, ino: u64) {
        let position = (ino - 1) as usize;
        self.super_block[position] = None;
    }

    /*
        Función que elimina una referencia dentro del inode
        E: ino (identificador del inode) y reference (identificador del inode referenciado)
        S: N/A
    */
    fn remove_reference(&mut self, ino: u64, reference: usize) {
        let position = (ino - 1) as usize;
        let inode: &mut Option<Inode> = &mut self.super_block[position];

        match inode {
            Some(inode) => {
                let mut reference_position: Option<usize> = Option::None;

                for i in 0..inode.references.len() {
                    if inode.references[i].unwrap() == reference {
                        reference_position = Some(i);
                        break;
                    }
                }

                match reference_position {
                    Some(refer) => inode.references[refer] = None,
                    None => panic!("Error: referencia no encontrada")
                }
            },
            None => panic!("Error: Inode no existe en el contexto actual")
        }
    }

    /*
        Función que regresa el inode solicitado pero mutable
        E: ino (identificador del inode)
        S: un opcional del inode solicitado mutable
    */
    fn get_inode_mutable(&mut self, ino: u64) -> Option<&mut Inode> {
        let position = (ino as usize) - 1;

        match &mut self.super_block[position] {
            Some(inode) => Some(inode),
            None => None
        }
    }

    /*
        Función que regresa el inode solicitado
        E: ino (identificador del inode)
        S: un opcional del inode solicitado
    */
    fn get_inode(&self, ino: u64) -> Option<&Inode> {
        let position = (ino as usize) - 1;

        match &self.super_block[position] {
            Some(inode) => Some(inode),
            None => None
        }
    }

    /*
        Función que encuentra un inode por el nombre
        E: parent_ino (identificador del inode padre) y name (nombre del archivo o carpeta)
        S: un opcional del inode solicitado
    */
    fn find_inode_name(&self, parent_ino: u64, name: &str) -> Option<&Inode> {
        let position = (parent_ino as usize) - 1;
        let parent_inode = &self.super_block[position];

        match parent_inode {
            Some(parent_inode) => {
                for referenced_inode in parent_inode.references.iter(){
                    if let Some(ino) = referenced_inode {
                        let position = (ino.clone() as usize) - 1;
                        let reference = &self.super_block[position];

                        match reference {
                            Some(inode) => {
                                let name_inode:&str = inode.name.as_str();
                                let requested_name = name.trim();
                                
                                if name_inode == requested_name {
                                    return Some(inode);
                                }
                            },
                            None => panic!("Error: No se encontró el inode")
                        }
                    }
                }
            },
            None => panic!("Error: No se encontró el inode parent")
        }

        return None;
    }

    /*
        Función que regresa las referencias de un inode
        E: ino (identificador del inode)
        S: un opcional de las referencias del inode
    */
    fn get_references(&self, ino: u64) -> &Vec<Option<usize>> {
        let position = (ino as usize) - 1;

        match &self.super_block[position] {
            Some(inode) => &inode.references,
            None => panic!("Error: No se encontró el inode para referencias")
        }
    }

    /*
        Función que regresa los bytes del contenido de un memory block
        E: block_position (la ubicación del bloque solicitado)
        S: un opcional del contenido del memory block como bytes
    */
    fn get_content_bytes(&self, block_position: usize) -> &Option<Vec<u8>> {
        let memory_block = &self.memory_blocks[block_position];
        return &memory_block.data;
    }

    /*
        Función que guarda el contenido en un memory block
        E: block_position (la ubicación del bloque solicitado) y el contenido (bytes)
        S: N/A
    */
    fn write_content_bytes(&mut self, block_position: usize, content: Vec<u8>) {
        if content.len() > self.block_size {
            panic!("Error: el contenido es muy grande");
        }

        let memory_block = MemoryBlock{ data: Some(content)};
        self.memory_blocks[block_position] = memory_block;
    }

    /*
        Función que guardan una referencia en un inode
        E: ino (identificador del inode), reference (ubicación de la referencia) y value (ino de la referencia)
        S: N/A
    */
    fn write_reference(&mut self, ino: u64, reference: usize, value: usize) {
        let position = (ino as usize) - 1;
        match &mut self.super_block[position] {
            Some(inode) => {
                inode.references[reference] = Some(value)
            },
            None => panic!("Error: no se encontro el inode para insertar la referencia")
        }
    }

    /*
        Función que codifica los memory_blocks
        E: N/A
        S: un arreglo de bytes que representa los memory blocks codificados
    */
    fn encode_memory_blocks(&self,) -> Vec<u8> {
        serialize(&self.memory_blocks).unwrap()
    }

    /*
        Función que codifica un inode
        E: inode (inode a codificar)
        S: un arreglo de bytes (un inode codificado)
    */
    fn encode_inode(&self, inode: &Inode) -> Vec<u8> {
        serialize(inode).unwrap()
    }

    /*
        Función que codifica todos los inodes del disco
        E: N/A
        S: un arreglo de arreglos de bytes (todos los inodes codificado)
    */
    fn encode_inodes(&self) -> Vec<Vec<u8>> {
        let mut result: Vec<Vec<u8>> = Vec::new();
        
        for inode in self.super_block.iter() {
            match inode {
                Some(inode) => {
                    if inode.attributes.ino != 1 {
                        result.push(self.encode_inode(inode));
                    }
                },
                None => ()
            }
        }

        result
    }

    /*
        Función que decodifica los memory_blocks
        E: un arreglo de bytes que representan los memory blocks codificado
        S: un arreglo de MemoryBlock
    */
    fn decode_memory_blocks(&self, memory_blocks: &Vec<u8>) -> Vec<MemoryBlock>{
        deserialize(&memory_blocks[..]).unwrap()
    }

    /*
        Función que decodifica un inode
        E: inode (arreglo de bytes => inode codificado)
        S: un inode (un inode decodificado)
    */
    fn decode_inode(&self, inode: &Vec<u8>) -> Inode {
        deserialize(&inode[..]).unwrap()
    }

    /*
        Función que decodifica todos los inodes
        E: todos los inodes(un arreglo de arreglos de bytes => inodes codificados)
        S: un arreglo de inodes decodificados
    */
    fn decode_inodes(&self, inodes: Vec<Vec<u8>>) -> Vec<Inode> {
        let mut result: Vec<Inode> = Vec::new();
        
        for inode in inodes.iter() {
            result.push(self.decode_inode(inode));
        }

        result
    }

    /*
        Función que guarda los inodes y llama a la función para convertirlos en QR
        E: N/A
        S: N/A
    */
    fn save_inodes(&self) {
        self.transform_inodes_to_qr(self.encode_inodes());
    }

    /*
        Función que transforma los inodes codificados en QR
        E: un arreglo de bytes que representa los inodes codificados
        S: N/A
    */
    fn transform_inodes_to_qr(&self, inodes: Vec<Vec<u8>>) {
        let mut counter = 0;
        for inode in inodes.iter() {
            let qr_code = QrCode::new(inode).unwrap();
            let image_qr = qr_code.render::<Luma<u8>>().build();
            let path = format!("/home/kzumbado/Documents/proyecto2-sistemas-operativos/qrfs/qr_codes/inode{}.png", counter);
            image_qr.save(path);
            counter = counter + 1;
        }
    }

    /*
        Función que convierte los QR a inodes
        E: un arreglo con los paths a los archivos seleccionados por el usuario
        S: un arreglo de opcionales de inodes (super_block)
    */
    fn translate_inodes_qr(&self, paths: Vec<PathBuf>) -> Vec<Option<Inode>> {
        let mut super_block:Vec<Option<Inode>> = Vec::new();
        for inode in paths.iter() {
            let image_qr = image::open(inode).unwrap();
            let gray_image = image_qr.into_luma8();

            let mut decoder = quircs::Quirc::default();

            let with_image = gray_image.width() as usize;
            let height_image = gray_image.height() as usize;
            let data = decoder.identify(with_image, height_image, &gray_image);

            let mut result: Option<Vec<u8>> = Option::None;
            for element in data {
                let code = element.expect("Error: no se pudo identificar el QR");
                let decoded = code.decode().expect("Error: no se pudo decodicar el QR");
                result = Some(decoded.payload);
            }

            match result {
                Some(result) => {
                    let inode: Inode = self.decode_inode(&result);
                    super_block.push(Some(inode));
                },
                None => panic!("Error: no se pudo transformar a Inode")
            }

        }

        super_block
    }

    /*
        Función que despliega un dialogo de selección de archivos al usuario
        E: N/A
        S: un arreglo con los paths a los archivos seleccionados por el usuario
    */
    fn display_dialog(&self) -> Vec<PathBuf> {
        let paths = FileDialog::new()
            .set_location("~/Documents/proyecto2-sistemas-operativos/qrfs/qr_codes")
            .add_filter("PNG Image", &["png"])
            .show_open_multiple_file()
            .unwrap();

        println!("El path es {:?}", paths);
        paths
    }

}

/*
    Estructura de inode para guardar los datos de los archivos y carpetas
    implementa serialize y deserialize
*/
#[derive(Serialize, Deserialize)]
struct Inode {
    name: String,
    #[serde(with = "FileAttrDef")]
    attributes: FileAttr,
    references: Vec<Option<usize>>
}

/*
    Estructura de inode para guardar los datos de los archivos y carpetas
    implementa serialize y deserialize
*/
#[derive(Serialize, Deserialize)]
struct MemoryBlock {
    data: Option<Vec<u8>>
}

/*
    Estructura auxiliar de Timespec
    es necesaria ya que serialize y deserialize no puede tener estructuras dentro de estructuras
    implementa serialize y deserialize
*/
#[derive(Serialize, Deserialize)]
#[serde(remote = "Timespec")]
pub struct TimespecDef {
    pub sec: i64,
    pub nsec: i32,
}

/*
    Estructura auxiliar de FileType
    es necesaria ya que serialize y deserialize no puede tener estructuras dentro de estructuras
    implementa serialize y deserialize
*/
#[derive(Serialize, Deserialize)]
#[serde(remote = "FileType")]
pub enum FileTypeDef {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Directory,
    RegularFile,
    Symlink,
    Socket,
}

/*
    Estructura auxiliar de FileAttr
    es necesaria ya que serialize y deserialize no puede tener estructuras dentro de estructuras
    implementa serialize y deserialize
*/
#[derive(Serialize, Deserialize)]
#[serde(remote = "FileAttr")]
pub struct FileAttrDef {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    #[serde(with = "TimespecDef")]
    pub atime: Timespec,
    #[serde(with = "TimespecDef")]
    pub mtime: Timespec,
    #[serde(with = "TimespecDef")]
    pub ctime: Timespec,
    #[serde(with = "TimespecDef")]
    pub crtime: Timespec,
    #[serde(with = "FileTypeDef")]
    pub kind: FileType,
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub flags: u32,
}

/*
    Estructura del filesystem
*/
struct QRFS {
    disk: Disk
}

impl QRFS {
    /*
        Función que crea un nuevo QRFS
        E: el path raíz
        S: una estructura QRFS
    */
    fn new(root_path: String) -> Self {
        let max_files: usize = DEFAULT_SIZE;
        let memory_size: usize = DEFAULT_SIZE * DEFAULT_SIZE * DEFAULT_SIZE;
        let block_size: usize = max_files * (mem::size_of::<Vec<Inode>>() + mem::size_of::<Inode>());
        
        initscr();
        addstr("¿Desea seleccionar archivos previos? \nY = sí\nCualquiera = no\n");
        
        refresh();
        
        let response = getch();
        clear();
        endwin();
        let mut disk = Disk::new(root_path, memory_size, block_size);

        if response == 89 {
            let paths = disk.display_dialog();
            let inodes = disk.translate_inodes_qr(paths);

            for inode in inodes {
                match inode {
                    Some(inode) => {
                        let position = disk.find_empty_reference(1);
                        match position {
                            Some(pos) => {
                                let ino = inode.attributes.ino as usize;
                                let content: Vec<u8> = Vec::default();

                                disk.write_inode(inode);
                                disk.write_reference(1, pos, ino);
                                disk.write_content_bytes(ino - 1, content);
                            },
                            None => println!("Error: fallo al inicializar el fs")
                        }
                    },
                    None => panic!("Error: el inode no es válido")
                }
            }

            println!("Exito");
        }

        QRFS { disk }
    }
}

impl Drop for QRFS {
    /*
        Función drop del filesystem
        
        sirve para guardar los archivos en caso de que el usuario desee hacerlo
    */
    fn drop(&mut self) {

        initscr();
        addstr("¿Desea guardar los archivos? \nY = sí\nCualquiera = no\n");
        
        refresh();
        
        let response = getch();
        
        endwin();
        
        if response == 89 {
            println!("\nGuardando inodes");
            &self.disk.save_inodes();
            println!("Guardados correctamente\n");
        }
    }
}

impl Filesystem for QRFS {
    /*
        Función lookup del filesystem
        
        sirve para detectar los archivos o carpetas pertenecientes del fs
    */
    fn lookup(&mut self, _req: &Request, parent: u64, name: &std::ffi::OsStr, reply: ReplyEntry) {
        println!("Operation: lookup");

        let inode = self.disk.find_inode_name(parent, name.to_str().unwrap());

        match inode {
            Some(inode) => {
                let ttl = get_time();
                reply.entry(&ttl, &inode.attributes, 0);
            },
            None => reply.error(ENOENT)
        }
    }

    /*
        Función getattr del filesystem
        
        sirve para obtener los atributos de un archivo o carpeta
    */
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("Operation: getattr");

        match self.disk.get_inode(ino) {
            Some(inode) => {
                let ttl = get_time();
                reply.attr(&ttl, &inode.attributes);
            },
            None => reply.error(ENOENT)
        }
    }

    /*
        Función read del filesystem
        
        sirve para leer el contenido de un archivo del fs
    */
    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, _offset: i64, _size: u32, reply: ReplyData) {
        println!("Operation: read");

        let memory_block = self.disk.get_content_bytes((ino - 1)  as usize);
    
        match memory_block {
            Some(memory_block) => {
                reply.data(memory_block)
            },
            None => reply.error(EIO)
        }
    }

    /*
        Función readdir del filesystem
        
        sirve para detectar los directorios del fs
    */
    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        println!("Operation: readdir");

        if ino == 1 {
            if offset == 0 {
                reply.add(1, 0, FileType::Directory, ".");
                reply.add(1, 1, FileType::Directory, "..");
            }
        }

        let inode: Option<&Inode> = self.disk.get_inode(ino);

        if mem::size_of_val(&inode) == offset as usize {
            reply.ok();
            return;
        }

        match inode {
            Some(inode) => {
                let references = &inode.references;

                for ino in references.iter() {
                    if let Some(ino) = ino {
                        let inode = self.disk.get_inode(*ino as u64);

                        if let Some(data) = inode {
                            if data.attributes.ino == 1 {
                                continue;
                            }

                            let name = data.name.clone();
                            let offset = mem::size_of_val(&inode) as i64;
                            reply.add(data.attributes.ino, offset, data.attributes.kind, name);
                        }
                    }
                }
                reply.ok();
            },
            None => reply.error(ENOENT)
        }
    }

    /*
        Función mkdir del filesystem
        
        sirve para crear una nueva carpeta en el fs
    */
    fn mkdir(&mut self, _req: &Request, parent: u64, name: &OsStr, _mode: u32, reply: ReplyEntry) {
        println!("Operation: mkdir");

        let reference_position = self.disk.find_empty_reference(parent);

        match reference_position {
            Some(position) => {
                let ino = self.disk.find_next_ino();

                match ino {
                    Some(ino) => {
                        let ttl = get_time();
                        let attributes = FileAttr {
                            ino: ino as u64,
                            size: 0,
                            blocks: 1,
                            atime: ttl,
                            mtime: ttl,
                            ctime: ttl,
                            crtime: ttl,
                            kind: FileType::Directory,
                            perm: 0o755,
                            nlink: 0,
                            uid: unsafe {getuid()},
                            gid: unsafe {getgid()},
                            rdev: 0,
                            flags: 0
                        };

                        let name:String = name.to_str().unwrap().to_owned();

                        let inode = Inode {
                            name,
                            attributes,
                            references: vec![Option::None; MAX_FILES_DIRECTORY]
                        };

                        self.disk.write_inode(inode);
                        self.disk.write_reference(parent, position, ino as usize);

                        reply.entry(&ttl, &attributes, 0);

                    },
                    None => reply.error(ENOSPC)
                }
            },
            None => println!("Disco lleno")
        }
    }

    /*
        Función create del filesystem
        
        sirve para crear un nuevo archivo en el fs
    */
    fn create(&mut self, _req: &Request, parent: u64, name: &OsStr, _mode: u32, flags: u32, reply: ReplyCreate) {
        println!("Operation: create");

        let reference_position = self.disk.find_empty_reference(parent);

        if reference_position == None {
            println!("Error: No es posible crear más archivos");
            reply.error(EIO);
            return ;
        }

        let next_ino = self.disk.find_next_ino();
        let memory_block = self.disk.find_empty_memory_block();

        if next_ino == None || memory_block == None {
            println!("Error: No queda espacio disponible");
            reply.error(ENOSPC);
            return ;
        }

        let next_ino = next_ino.unwrap();
        let memory_block = memory_block.unwrap();

        let ttl = get_time();

        let attributes = FileAttr {
            ino: next_ino,
            size: 0,
            blocks: 1,
            atime: ttl,
            mtime: ttl,
            ctime: ttl,
            crtime: ttl,
            kind: FileType::RegularFile,
            perm: 0o755,
            nlink: 0,
            uid: unsafe {getuid()},
            gid: unsafe {getgid()},
            rdev: 0,
            flags
        };

        let name = name.to_str().unwrap().to_owned();

        let mut inode = Inode {
            name,
            attributes,
            references: vec![Option::None; MAX_FILES_DIRECTORY]
        };

        inode.references[0] = Some(memory_block);
        let content: Vec<u8> = Vec::default();

        self.disk.write_inode(inode);
        self.disk.write_content_bytes(memory_block, content);

        let reference_position = reference_position.unwrap();
        self.disk.write_reference(parent, reference_position, next_ino as usize);
        
        reply.created(&ttl, &attributes, 1, next_ino, flags);
    }

    /*
        Función open del filesystem
        
        sirve para abrir un archivo del fs
    */
    fn open(&mut self, _req: &Request, ino: u64, flags: u32, reply: ReplyOpen) {
        println!("Operation: open");

        let inode = self.disk.get_inode(ino);

        match inode {
            Some(_) => reply.opened(ino, flags),
            None => reply.error(ENOSYS)
        }
    }
    
    /*
        Función write del filesystem
        
        sirve para escribir dentro de un archivo del fs
    */
    fn write(&mut self, _req: &Request, ino: u64, _fh: u64, _offset: i64, data: &[u8], _flags: u32, reply: ReplyWrite) {
        println!("Operation: write");

        let inode = self.disk.get_inode_mutable(ino);
        let content: Vec<u8> = data.to_vec();

        match inode {
            Some(inode) => {
                inode.attributes.size = data.len() as u64;

                let position = (ino as usize) - 1;

                self.disk.write_content_bytes(position, content);
                reply.written(data.len() as u32);
            },
            None => {
                println!("Error: No se encontró el inode");
                reply.error(ENOENT);
            }
        }
    }

    /*
        Función rmdir del filesystem
        
        sirve para eliminar una carpeta del fs
    */
    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_str().unwrap();
        let inode = self.disk.find_inode_name(parent, name);
        
        match inode {
            Some(inode) => {
                let ino = inode.attributes.ino;
                self.disk.remove_reference(parent, ino as usize);
                self.disk.remove_inode(ino);

                reply.ok();
            },
            None => reply.error(EIO)
        }
    }

    /*
        Función statfs del filesystem
        
        sirve para desplegar las estadísticas del fs
    */
    fn statfs(&mut self, _req: &Request, _ino: u64, reply: fuse::ReplyStatfs) {
        println!("Operation: statfs");

        let blocks = self.disk.memory_blocks.len();
        let bfree = blocks - (self.disk.find_empty_memory_block().unwrap());
        let bavail = bfree;
        let bsize = self.disk.block_size;
        let files = self.disk.find_empty_memory_block().unwrap();
        let namelen = NAMELEN;
        let ffree = self.disk.max_files - files;
        let frsize = blocks as u32;

        reply.statfs(
            blocks as u64, 
            bfree as u64, 
            bavail as u64, 
            files as u64, 
            ffree as u64, 
            bsize as u32, 
            namelen, 
            frsize);
    }

    /*
        Función rename del filesystem
        
        sirve para renombrar o editar un archivo del fs
    */
    fn rename(&mut self, _req: &Request, parent: u64, name: &OsStr, _newparent: u64, newname: &OsStr, reply: ReplyEmpty) {
        println!("Operation: rename");

        let name = name.to_str().unwrap();
        let inode = self.disk.find_inode_name(parent, name);

        match inode {
            Some(inode) => {
                let ino = inode.attributes.ino;
                let new_inode = self.disk.get_inode_mutable(ino);
                
                match new_inode {
                    Some(inode) => {
                        let newname = newname.to_str().unwrap().to_owned();
                        inode.name = newname;
                        reply.ok();
                    },
                    None => panic!("Error: No se pudo renombrar el archivo")
                }
            },
            None => reply.error(ENOENT)
        }
    }

    /*
        Función access del filesystem
        
        sirve para checar por los permisos de un archivo de fs
    */
    fn access(&mut self, _req: &Request, _ino: u64, _mask: u32, reply: ReplyEmpty) {
        println!("Operation: access");
        reply.ok();
    }

    /*
        Función fsync del filesystem
        
        sirve para sincronizar los archivos del fs
    */
    fn fsync(&mut self, _req: &Request, _ino: u64, _fh: u64, _datasync: bool, reply: ReplyEmpty) {
        println!("Operation: fsync");
        reply.error(ENOSYS);
    }

    /*
        Función opendir del filesystem
        
        sirve para abrir una carpeta del fs
    */
    fn opendir(&mut self, _req: &Request, ino: u64, flags: u32, reply: ReplyOpen) {
        println!("Operation: opendir");
        
        let inode = self.disk.get_inode(ino);

        match inode {
            Some(inode) => reply.opened(inode.attributes.ino, flags),
            None => reply.error(EISDIR)
        }
    }
}

fn main() {
    let mountpoint = match env::args().nth(1) {
        Some(path) => path,
        None => {
            println!("Error: se debe ingresar un mountpoint");
            return ;
        }
    };

    let fs = QRFS::new(mountpoint.clone());

    let options = ["-o", "nonempty"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();
    
    println!("QRFS iniciado");
    
    fuse::mount(fs, &mountpoint, &options);
}