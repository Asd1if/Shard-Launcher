//! Минимальный NBT без сжатия — ровно столько, чтобы дописать сервер в `servers.dat`.
//! Неизвестные теги читаются и записываются как есть, чужие записи не теряются.

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// Тип элементов и сами элементы
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| Error::msg("nbt: unexpected end"))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().expect("2")))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().expect("4")))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.i32()?).map_err(|_| Error::msg("nbt: negative length"))
    }
    fn string(&mut self) -> Result<String> {
        let n = self.i16()? as u16 as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn payload(&mut self, id: u8, depth: u32) -> Result<Tag> {
        if depth > 64 {
            return Err(Error::msg("nbt: too deep"));
        }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let item = self.u8()?;
                let n = self.len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.payload(item, depth + 1)?);
                }
                Tag::List(item, items)
            }
            10 => {
                let mut items = vec![];
                loop {
                    let id = self.u8()?;
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    items.push((name, self.payload(id, depth + 1)?));
                }
                Tag::Compound(items)
            }
            11 => {
                let n = self.len()?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_>>()?)
            }
            12 => {
                let n = self.len()?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_>>()?)
            }
            _ => return Err(Error::msg(format!("nbt: unknown tag {id}"))),
        })
    }
}

/// Корневой тег файла (имя корня игнорируется).
pub fn read(data: &[u8]) -> Result<Tag> {
    let mut r = Reader { data, pos: 0 };
    let id = r.u8()?;
    if id != 10 {
        return Err(Error::msg("nbt: root is not a compound"));
    }
    r.string()?;
    r.payload(10, 0)
}

pub fn write(root: &Tag) -> Vec<u8> {
    let mut out = vec![10];
    put_string(&mut out, "");
    put_payload(&mut out, root);
    out
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(u16::MAX as usize);
    out.extend((n as u16).to_be_bytes());
    out.extend(&bytes[..n]);
}

fn put_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend(v.to_be_bytes()),
        Tag::Int(v) => out.extend(v.to_be_bytes()),
        Tag::Long(v) => out.extend(v.to_be_bytes()),
        Tag::Float(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            out.extend(v.iter().map(|&b| b as u8));
        }
        Tag::String(v) => put_string(out, v),
        Tag::List(item, items) => {
            out.push(if items.is_empty() { *item } else { items[0].id() });
            out.extend((items.len() as i32).to_be_bytes());
            items.iter().for_each(|t| put_payload(out, t));
        }
        Tag::Compound(items) => {
            for (name, value) in items {
                out.push(value.id());
                put_string(out, name);
                put_payload(out, value);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|x| out.extend(x.to_be_bytes()));
        }
        Tag::LongArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|x| out.extend(x.to_be_bytes()));
        }
    }
}

/// Добавляет сервер в начало списка `servers.dat`, если такого адреса там ещё нет.
/// Возвращает false, если сервер уже был в списке.
pub fn add_server(path: &std::path::Path, name: &str, ip: &str) -> Result<bool> {
    let mut root = match std::fs::read(path) {
        Ok(bytes) => read(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Tag::Compound(vec![]),
        Err(e) => return Err(e.into()),
    };
    let Tag::Compound(items) = &mut root else { unreachable!("read returns a compound") };
    if !items.iter().any(|(k, _)| k == "servers") {
        items.push(("servers".into(), Tag::List(10, vec![])));
    }
    let (_, servers) = items.iter_mut().find(|(k, _)| k == "servers").expect("just added");
    let Tag::List(_, list) = servers else { return Err(Error::msg("nbt: servers is not a list")) };
    if list.iter().any(|s| s.get("ip") == Some(&Tag::String(ip.to_owned()))) {
        return Ok(false);
    }
    list.insert(0, Tag::Compound(vec![("name".into(), Tag::String(name.to_owned())), ("ip".into(), Tag::String(ip.to_owned()))]));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, write(&root))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_server_and_keeps_others() {
        let dir = std::env::temp_dir().join(format!("shard-nbt-{}", uuid::Uuid::new_v4()));
        let path = dir.join("servers.dat");
        assert!(add_server(&path, "Friends", "play.example.com").unwrap());
        assert!(add_server(&path, "Мой сервер", "localhost").unwrap());
        assert!(!add_server(&path, "Again", "localhost").unwrap());
        let root = read(&std::fs::read(&path).unwrap()).unwrap();
        let Some(Tag::List(10, list)) = root.get("servers") else { panic!("no list") };
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].get("name"), Some(&Tag::String("Мой сервер".into())));
        assert_eq!(list[1].get("ip"), Some(&Tag::String("play.example.com".into())));
        // Запись и чтение не меняют данные
        assert_eq!(read(&write(&root)).unwrap(), root);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
