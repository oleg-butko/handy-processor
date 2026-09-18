//! A temporary clipboard replacement, confined to the paste worker thread.
//! Snapshot every format before changing anything; never replace a newer copy.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL, HWND,
        },
        Graphics::Gdi::{
            CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject, HENHMETAFILE,
            HGDIOBJ,
        },
        System::{
            DataExchange::*,
            Memory::*,
            Ole::{OleDuplicateData, CF_UNICODETEXT, CLIPBOARD_FORMAT},
        },
    },
};

struct OpenClipboardGuard;
impl OpenClipboardGuard {
    fn acquire(owner: HWND) -> Result<Self, String> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match unsafe { OpenClipboard(Some(owner)) } {
                Ok(()) => return Ok(Self),
                Err(error) if Instant::now() >= deadline => {
                    return Err(format!("Буфер обмена занят: {error}"))
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

struct SavedFormat {
    format: u32,
    handle: Option<HANDLE>,
}

impl SavedFormat {
    // OleDuplicateData knows bitmap, palette and metafile-picture handles.
    // Enhanced metafiles need CopyEnhMetaFile instead of byte-wise duplication.
    fn duplicate(format: u32, source: HANDLE) -> Result<Self, String> {
        let base = match format {
            130 => 2,
            131 => 3,
            142 => 14, // CF_DSP* aliases
            other => other,
        };
        if base == 128 || (0x200..=0x3ff).contains(&base) {
            return Err(format!(
                "Нельзя сохранить специальный формат буфера {format}; вставка отменена"
            ));
        }
        let handle = unsafe {
            if base == 14 {
                HANDLE(CopyEnhMetaFileW(HENHMETAFILE(source.0), PCWSTR::null()).0)
            } else {
                OleDuplicateData(source, CLIPBOARD_FORMAT(base as u16), GMEM_MOVEABLE)
            }
        };
        if handle.is_invalid() {
            return Err(format!(
                "Не удалось сохранить формат буфера {format}; вставка отменена"
            ));
        }
        Ok(Self {
            format,
            handle: Some(handle),
        })
    }

    fn text(text: &str) -> Result<Self, String> {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let memory =
            unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) }.map_err(|e| e.to_string())?;
        let value = Self {
            format: CF_UNICODETEXT.0 as u32,
            handle: Some(HANDLE(memory.0)),
        };
        let data = unsafe { GlobalLock(memory) } as *mut u16;
        if data.is_null() {
            return Err("Не удалось подготовить текст в буфере".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), data, wide.len());
            let _ = GlobalUnlock(memory);
        }
        Ok(value)
    }

    fn publish(&mut self) -> Result<(), String> {
        if let Some(handle) = self.handle {
            unsafe { SetClipboardData(self.format, Some(handle)) }.map_err(|e| e.to_string())?;
            self.handle = None; // Windows now owns this handle.
        }
        Ok(())
    }
}

impl Drop for SavedFormat {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        unsafe {
            match self.format {
                2 | 9 | 130 => {
                    let _ = DeleteObject(HGDIOBJ(handle.0));
                }
                14 | 142 => {
                    let _ = DeleteEnhMetaFile(Some(HENHMETAFILE(handle.0)));
                }
                3 | 131 => {
                    let memory = HGLOBAL(handle.0);
                    let picture = GlobalLock(memory) as *const METAFILEPICT;
                    if !picture.is_null() {
                        let _ = DeleteMetaFile((*picture).hMF);
                        let _ = GlobalUnlock(memory);
                    }
                    let _ = GlobalFree(Some(memory));
                }
                _ => {
                    let _ = GlobalFree(Some(HGLOBAL(handle.0)));
                }
            }
        }
    }
}

fn snapshot() -> Result<Vec<SavedFormat>, String> {
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            if unsafe { GetLastError() } != ERROR_SUCCESS {
                return Err("Не удалось перечислить форматы буфера; вставка отменена".into());
            }
            return Ok(formats);
        }
        let source = unsafe { GetClipboardData(format) }
            .map_err(|e| format!("Не удалось сохранить формат буфера {format}: {e}"))?;
        formats.push(SavedFormat::duplicate(format, source)?);
    }
}

fn publish_all(formats: &mut [SavedFormat]) -> Result<(), String> {
    let mut error = None;
    for format in formats {
        if let Err(failure) = format.publish() {
            error.get_or_insert(failure);
        }
    }
    error.map_or(Ok(()), Err)
}

pub struct TemporaryClipboard {
    owner: HWND,
    active: bool,
    sequence: Option<u32>,
    marker_format: u32,
    marker: Vec<u16>,
    formats: Vec<SavedFormat>,
}

impl TemporaryClipboard {
    pub fn new(owner: usize, text: &str) -> Result<Self, String> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let owner = HWND(owner as *mut _);
        let mut replacement = SavedFormat::text(text)?;
        let marker_text = format!(
            "{}:{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let mut marker = SavedFormat::text(&marker_text)?;
        marker.format =
            unsafe { RegisterClipboardFormatW(w!("HandyProcessor.TemporaryClipboard")) };
        if marker.format == 0 {
            return Err("Не удалось подготовить временный буфер".into());
        }
        let open = OpenClipboardGuard::acquire(owner)?;
        let mut formats = snapshot()?;
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        if let Err(error) = replacement.publish().and_then(|()| marker.publish()) {
            // The text may already have been published when the marker failed.
            unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
            return match publish_all(&mut formats) {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}. Не удалось восстановить буфер: {restore}")),
            };
        }
        let mut clipboard = Self {
            owner,
            active: true,
            sequence: None,
            marker_format: marker.format,
            marker: marker_text.encode_utf16().chain(Some(0)).collect(),
            formats,
        };
        // CloseClipboard adds synthesized text formats and can advance the
        // sequence. Reopen and check our unique marker before recording it:
        // a copy made between closing and reopening must never be overwritten.
        drop(open);
        {
            let _open = OpenClipboardGuard::acquire(owner)?;
            if !clipboard.has_marker() {
                clipboard.active = false;
                return Err("Буфер изменился во время подготовки вставки".into());
            }
            clipboard.sequence = Some(unsafe { GetClipboardSequenceNumber() });
        }
        Ok(clipboard)
    }

    // Requires the clipboard to be open on this thread.
    fn has_marker(&self) -> bool {
        unsafe {
            if GetClipboardOwner().ok() != Some(self.owner) {
                return false;
            }
            let Ok(handle) = GetClipboardData(self.marker_format) else {
                return false;
            };
            let memory = HGLOBAL(handle.0);
            if GlobalSize(memory) < self.marker.len() * 2 {
                return false;
            }
            let pointer = GlobalLock(memory) as *const u16;
            if pointer.is_null() {
                return false;
            }
            let matches = std::slice::from_raw_parts(pointer, self.marker.len()) == self.marker;
            let _ = GlobalUnlock(memory);
            matches
        }
    }

    pub fn verify(&self) -> Result<(), String> {
        if self.sequence == Some(unsafe { GetClipboardSequenceNumber() }) {
            Ok(())
        } else {
            Err("Содержимое буфера изменилось; автоматическая вставка отменена".into())
        }
    }

    /// Returns false if another writer replaced our text: its copy wins.
    pub fn restore(&mut self) -> Result<bool, String> {
        if !self.active {
            return Ok(false);
        }
        let _open = OpenClipboardGuard::acquire(self.owner)?;
        if (self.sequence.is_some() && self.verify().is_err()) || !self.has_marker() {
            self.active = false;
            return Ok(false);
        }
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        self.active = false;
        publish_all(&mut self.formats)?;
        Ok(true)
    }
}

impl Drop for TemporaryClipboard {
    fn drop(&mut self) {
        // Also attempt restoration on an early return or unwinding worker.
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{core::w, Win32::UI::WindowsAndMessaging::*};

    #[test]
    fn snapshot_duplicates_unicode_without_losing_spaces_or_hyphens() {
        let original = SavedFormat::text("Добрый день — Санкт-Петербург").unwrap();
        let duplicate = SavedFormat::duplicate(original.format, original.handle.unwrap()).unwrap();
        drop(original);
        let memory = HGLOBAL(duplicate.handle.unwrap().0);
        unsafe {
            let pointer = GlobalLock(memory) as *const u16;
            assert!(!pointer.is_null());
            let expected: Vec<_> = "Добрый день — Санкт-Петербург"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            assert_eq!(
                std::slice::from_raw_parts(pointer, expected.len()),
                expected
            );
            let _ = GlobalUnlock(memory);
        }
    }

    // Explicit opt-in: this test temporarily owns the real desktop clipboard.
    #[test]
    #[ignore = "uses the Windows clipboard; do not copy anything while it runs"]
    fn restores_formats_empty_clipboard_and_failed_paste_but_preserves_new_copies() {
        struct Sandbox {
            owner: HWND,
            original: Vec<SavedFormat>,
        }
        impl Drop for Sandbox {
            fn drop(&mut self) {
                if let Ok(_open) = OpenClipboardGuard::acquire(self.owner) {
                    unsafe {
                        EmptyClipboard().unwrap();
                    }
                    publish_all(&mut self.original).unwrap();
                }
                unsafe {
                    let _ = DestroyWindow(self.owner);
                }
            }
        }
        fn replace(owner: HWND, mut formats: Vec<SavedFormat>) {
            let _open = OpenClipboardGuard::acquire(owner).unwrap();
            unsafe {
                EmptyClipboard().unwrap();
            }
            publish_all(&mut formats).unwrap();
        }
        fn read(owner: HWND, format: u32) -> Vec<u16> {
            let _open = OpenClipboardGuard::acquire(owner).unwrap();
            let handle = unsafe { GetClipboardData(format) }.unwrap();
            let memory = HGLOBAL(handle.0);
            unsafe {
                let pointer = GlobalLock(memory) as *const u16;
                assert!(!pointer.is_null());
                let length = (0..)
                    .take_while(|offset| *pointer.add(*offset) != 0)
                    .count();
                let text = std::slice::from_raw_parts(pointer, length).to_vec();
                let _ = GlobalUnlock(memory);
                text
            }
        }
        let owner = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Clipboard test"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        let original = {
            let _open = OpenClipboardGuard::acquire(owner).unwrap();
            snapshot().unwrap()
        };
        let _sandbox = Sandbox { owner, original };
        let text_format = CF_UNICODETEXT.0 as u32;
        let metadata_format =
            unsafe { RegisterClipboardFormatW(w!("HandyProcessor.ClipboardTest")) };
        assert_ne!(metadata_format, 0);

        replace(owner, vec![]);
        let mut temporary = TemporaryClipboard::new(owner.0 as usize, "временный").unwrap();
        temporary.verify().unwrap();
        assert!(temporary.restore().unwrap());
        {
            let _open = OpenClipboardGuard::acquire(owner).unwrap();
            assert!(snapshot().unwrap().is_empty());
        }

        let mut metadata = SavedFormat::text("метаданные").unwrap();
        metadata.format = metadata_format;
        replace(
            owner,
            vec![SavedFormat::text("исходный").unwrap(), metadata],
        );
        let temporary = TemporaryClipboard::new(owner.0 as usize, "вставка").unwrap();
        assert_eq!(
            read(owner, text_format),
            "вставка".encode_utf16().collect::<Vec<_>>()
        );
        drop(temporary); // Simulate an early return from a failed paste.
        assert_eq!(
            read(owner, text_format),
            "исходный".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            read(owner, metadata_format),
            "метаданные".encode_utf16().collect::<Vec<_>>()
        );

        let mut temporary = TemporaryClipboard::new(owner.0 as usize, "новый").unwrap();
        replace(owner, vec![SavedFormat::text("новый").unwrap()]);
        assert!(temporary.verify().is_err());
        assert!(!temporary.restore().unwrap());
        assert_eq!(
            read(owner, text_format),
            "новый".encode_utf16().collect::<Vec<_>>()
        );
    }
}
