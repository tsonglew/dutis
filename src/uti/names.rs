use std::collections::HashMap;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref UTI_FRIENDLY_NAMES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Video formats
        m.insert("public.mp4", "MP4 Video");
        m.insert("public.mpeg", "MPEG Video");
        m.insert("public.avi", "AVI Video");
        m.insert("public.mov", "QuickTime Movie");
        m.insert("com.apple.quicktime-movie", "QuickTime Movie");
        m.insert("public.mpeg-4", "MPEG-4 Video");

        // Audio formats
        m.insert("public.mp3", "MP3 Audio");
        m.insert("public.wav", "WAV Audio");
        m.insert("public.aiff", "AIFF Audio");
        m.insert("public.m4a", "M4A Audio");
        m.insert("com.apple.m4a-audio", "M4A Audio");
        m.insert("public.audio", "Audio");

        // Image formats
        m.insert("public.jpeg", "JPEG Image");
        m.insert("public.png", "PNG Image");
        m.insert("public.gif", "GIF Image");
        m.insert("com.apple.pict", "PICT Image");
        m.insert("public.svg-image", "SVG Image");
        m.insert("public.tiff", "TIFF Image");

        // Document formats
        m.insert("public.plain-text", "Plain Text");
        m.insert("public.text", "Text");
        m.insert("public.html", "HTML Document");
        m.insert("public.xml", "XML Document");
        m.insert("public.json", "JSON Document");
        m.insert("com.adobe.pdf", "PDF Document");
        m.insert("com.microsoft.word.doc", "Word Document");
        m.insert("org.openxmlformats.wordprocessingml.document", "Word Document");
        m.insert("public.rtf", "Rich Text Document");
        m.insert("public.markdown", "Markdown Document");

        // Programming languages
        m.insert("public.python-script", "Python Source");
        m.insert("public.javascript-source", "JavaScript Source");
        m.insert("public.ruby-script", "Ruby Source");
        m.insert("public.go-source", "Go Source");
        m.insert("public.rust-source", "Rust Source");
        m.insert("public.c-source", "C Source");
        m.insert("public.c-plus-plus-source", "C++ Source");
        m.insert("public.swift-source", "Swift Source");
        m.insert("public.java-source", "Java Source");
        m.insert("public.shell-script", "Shell Script");

        // Archive formats
        m.insert("public.zip-archive", "ZIP Archive");
        m.insert("org.gnu.gnu-zip-archive", "GZIP Archive");
        m.insert("public.tar-archive", "TAR Archive");
        m.insert("org.7-zip.7-zip-archive", "7Z Archive");
        m.insert("com.rarlab.rar-archive", "RAR Archive");

        m
    };

    pub static ref UTI_COMMON_SUFFIXES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Video formats
        m.insert("public.mp4", "mp4");
        m.insert("public.mpeg", "mpeg");
        m.insert("public.avi", "avi");
        m.insert("public.mov", "mov");
        m.insert("com.apple.quicktime-movie", "mov");
        m.insert("public.mpeg-4", "mp4");

        // Audio formats
        m.insert("public.mp3", "mp3");
        m.insert("public.wav", "wav");
        m.insert("public.aiff", "aiff");
        m.insert("public.m4a", "m4a");
        m.insert("com.apple.m4a-audio", "m4a");
        m.insert("public.audio", "audio");

        // Image formats
        m.insert("public.jpeg", "jpg");
        m.insert("public.png", "png");
        m.insert("public.gif", "gif");
        m.insert("com.apple.pict", "pict");
        m.insert("public.svg-image", "svg");
        m.insert("public.tiff", "tiff");

        // Document formats
        m.insert("public.plain-text", "txt");
        m.insert("public.text", "txt");
        m.insert("public.html", "html");
        m.insert("public.xml", "xml");
        m.insert("public.json", "json");
        m.insert("com.adobe.pdf", "pdf");
        m.insert("com.microsoft.word.doc", "doc");
        m.insert("org.openxmlformats.wordprocessingml.document", "docx");
        m.insert("public.rtf", "rtf");
        m.insert("public.markdown", "md");

        // Programming languages
        m.insert("public.python-script", "py");
        m.insert("public.javascript-source", "js");
        m.insert("public.ruby-script", "rb");
        m.insert("public.go-source", "go");
        m.insert("public.rust-source", "rs");
        m.insert("public.c-source", "c");
        m.insert("public.c-plus-plus-source", "cpp");
        m.insert("public.swift-source", "swift");
        m.insert("public.java-source", "java");
        m.insert("public.shell-script", "sh");

        // Archive formats
        m.insert("public.zip-archive", "zip");
        m.insert("org.gnu.gnu-zip-archive", "gz");
        m.insert("public.tar-archive", "tar");
        m.insert("org.7-zip.7-zip-archive", "7z");
        m.insert("com.rarlab.rar-archive", "rar");

        m
    };
}

pub fn get_friendly_name(uti: &str) -> String {
    UTI_FRIENDLY_NAMES.get(uti).copied().unwrap_or(uti).to_string()
}

pub fn get_common_suffix(uti: &str, input_suffix: &str) -> String {
    format!(".{}", input_suffix)
}
