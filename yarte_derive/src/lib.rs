#![allow(unused_imports, dead_code)]
use std::{
    collections::{BTreeMap, BTreeSet},
    iter,
    path::PathBuf,
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};

use syn::parse::ParseBuffer;
use syn::spanned::Spanned;
use yarte_codegen::{CodeGen, FmtCodeGen, HTMLCodeGen, TextCodeGen};
use yarte_helpers::{
    config::{get_source, read_config_file, Config, PrintConfig},
    logger::log,
};
use yarte_hir::{generate, visit_derive, HIROptions, Print, Struct};
use yarte_parser::{emitter, parse, parse_partials, source_map, Partial};

#[cfg(feature = "json")]
mod ser_json;

type Sources<'a> = &'a BTreeMap<PathBuf, String>;

macro_rules! build {
    ($i:ident, $codegen:ident, $opt:expr) => {{
        let config_toml: &str = &read_config_file();
        let config = &Config::new(config_toml);
        let s = &match visit_derive($i, config) {
            Ok(s) => s,
            Err(tt) => return tt.into(),
        };
        proc_macro2::fallback::force();
        let sources = &read(s.path.clone(), s.src.clone(), config);

        sources_to_tokens(sources, config, s, $codegen(s), $opt).into()
    }};
}

#[proc_macro_derive(TemplateText, attributes(template))]
/// Implements TemplateTrait without html escape functionality
pub fn template(input: TokenStream) -> TokenStream {
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(FmtCodeGen::new(TextCodeGen, s, "yarte"))
    }

    let i = &syn::parse(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            is_text: true,
            ..Default::default()
        }
    )
}

#[proc_macro_derive(Template, attributes(template))]
/// Implements TemplateTrait with html escape functionality
pub fn template_html(input: TokenStream) -> TokenStream {
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(FmtCodeGen::new(HTMLCodeGen, s, "yarte"))
    }
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(TemplateFixedText, attributes(template))]
#[cfg(feature = "fixed")]
/// Implements TemplateTrait without html escape functionality
pub fn template_ptr(input: TokenStream) -> TokenStream {
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::FixedCodeGen::new(
            yarte_codegen::TextFixedCodeGen("yarte"),
            s,
            "yarte",
        ))
    }

    let i = &syn::parse(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            is_text: true,
            ..Default::default()
        }
    )
}

#[proc_macro_derive(TemplateFixed, attributes(template))]
#[cfg(feature = "fixed")]
/// Implements TemplateFixedTrait with html escape functionality
pub fn template_html_ptr(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::FixedCodeGen::new(
            yarte_codegen::HTMLFixedCodeGen(PARENT),
            s,
            PARENT,
        ))
    }
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(TemplateBytesText, attributes(template))]
#[cfg(feature = "bytes-buf")]
/// Implements TemplateBytesTrait without html escape functionality
pub fn template_bytes(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let buf_i = format_ident!("bytes_mut");
    let buf: syn::Expr = syn::parse2(quote!(#buf_i)).unwrap();
    let get_codegen = |s| {
        Box::new(yarte_codegen::BytesCodeGen::new(
            yarte_codegen::TextBytesCodeGen::new(&buf),
            s,
            buf_i,
            PARENT,
        ))
    };

    let i = &syn::parse(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            is_text: true,
            ..Default::default()
        }
    )
}

#[proc_macro_derive(TemplateBytes, attributes(template))]
#[cfg(feature = "bytes-buf")]
/// Implements TemplateBytesTrait with html escape functionality
pub fn template_html_bytes(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let buf_i = format_ident!("bytes_mut");
    let buf: syn::Expr = syn::parse2(quote!(#buf_i)).unwrap();
    let get_codegen = |s| {
        Box::new(yarte_codegen::BytesCodeGen::new(
            yarte_codegen::HTMLBytesCodeGen::new(&buf),
            s,
            buf_i,
            PARENT,
        ))
    };
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(TemplateFixedMin, attributes(template))]
#[cfg(all(feature = "html-min", feature = "fixed"))]
/// # Work in Progress
/// Implements TemplateTrait with html minifier
pub fn template_html_min_ptr(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::FixedCodeGen::new(
            yarte_codegen::HTMLMinFixedCodeGen(PARENT),
            s,
            PARENT,
        ))
    }
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(TemplateBytesMin, attributes(template))]
#[cfg(all(feature = "html-min", feature = "bytes-buf"))]
/// # Work in Progress
/// Implements TemplateTrait with html minifier
pub fn template_html_min_bytes(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let buf_i = format_ident!("bytes_mut");
    let buf: syn::Expr = syn::parse2(quote!(#buf_i)).unwrap();
    let get_codegen = |s| {
        Box::new(yarte_codegen::BytesCodeGen::new(
            yarte_codegen::HTMLMinBytesCodeGen::new(&buf),
            s,
            buf_i,
            PARENT,
        ))
    };
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(TemplateMin, attributes(template))]
#[cfg(feature = "html-min")]
/// # Work in Progress
/// Implements TemplateTrait with html minifier
pub fn template_html_min(input: TokenStream) -> TokenStream {
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(FmtCodeGen::new(yarte_codegen::HTMLMinCodeGen, s, "yarte"))
    }
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

// TODO:
#[proc_macro_derive(App, attributes(template, msg, inner))]
#[cfg(feature = "wasm-app")]
pub fn app(input: TokenStream) -> TokenStream {
    fn get_codegen<'a>(s: &'a Struct) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::client::WASMCodeGen::new(s))
    }
    let i = &syn::parse(input).unwrap();
    let config_toml: &str = &read_config_file();
    let config = &Config::new(config_toml);
    let s = &match visit_derive(i, config) {
        Ok(s) => s,
        Err(tt) => return tt.into(),
    };
    // TODO: proc_macro2::fallback::force cause mismatch()
    let sources = &read(s.path.clone(), s.src.clone(), config);

    sources_to_tokens(sources, config, s, get_codegen(s), Default::default()).into()
}

// TODO:
#[proc_macro_derive(TemplateWasmServer, attributes(template))]
#[cfg(feature = "wasm-server")]
/// # Work in Progress
/// Implements TemplateTrait with wasm server behavior
///
/// Need additional `scrip` path argument attribute
pub fn template_wasm_server(input: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";
    let buf_i = format_ident!("bytes_mut");
    let buf: syn::Expr = syn::parse2(quote!(#buf_i)).unwrap();

    let get_codegen = |s| {
        Box::new(yarte_codegen::BytesCodeGen::new(
            yarte_codegen::server::WASMCodeGen::new(s, &buf),
            s,
            buf_i,
            PARENT,
        ))
    };
    let i = &syn::parse(input).unwrap();
    build!(i, get_codegen, Default::default())
}

#[proc_macro_derive(Serialize)]
#[cfg(feature = "json")]
pub fn serialize_json(i: TokenStream) -> TokenStream {
    let i = syn::parse(i).unwrap();
    let tokens = ser_json::serialize_json(i);
    tokens.into()
}

#[proc_macro]
/// Format handlebars string in this scope with html escape functionality
pub fn yformat_html(i: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";
    fn get_codegen<'a>(_s: &'a Struct<'a>) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::FnFmtCodeGen::new(HTMLCodeGen, PARENT))
    }

    let src: syn::LitStr = syn::parse(i).unwrap();
    let input = quote! {
        #[template(src = #src)]
        struct __YFormat__;
    };

    let i = &syn::parse2(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            resolve_to_self: false,
            parent: PARENT,
            ..Default::default()
        }
    )
}

#[proc_macro]
/// Format handlebars string in this scope without html escape functionality
pub fn yformat(i: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";
    fn get_codegen<'a>(_s: &'a Struct<'a>) -> Box<dyn CodeGen + 'a> {
        Box::new(yarte_codegen::FnFmtCodeGen::new(TextCodeGen, PARENT))
    }

    let src: syn::LitStr = syn::parse(i).unwrap();
    let input = quote! {
        #[template(src = #src)]
        struct __YFormat__;
    };

    let i = &syn::parse2(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            resolve_to_self: false,
            is_text: true,
            parent: PARENT,
        }
    )
}

#[proc_macro]
#[cfg(feature = "bytes-buf")]
/// Write handlebars template to `buf-min::Buffer` in this scope without html escape functionality
pub fn ywrite(i: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let ParseWriteArg { buf, src, .. } = match syn::parse(i) {
        Ok(arg) => arg,
        Err(e) => return e.to_compile_error().into(),
    };
    // Check correct syn::Expr
    {
        use syn::Expr::*;
        match &buf {
            Field(_) | Path(_) => (),
            _ => {
                return syn::Error::new(buf.span(), "first argument should be a ident or field")
                    .to_compile_error()
                    .into()
            }
        }
    }

    let get_codegen = |_| {
        Box::new(yarte_codegen::WriteBCodeGen::new(
            yarte_codegen::TextBytesCodeGen::new(&buf),
            PARENT,
        ))
    };

    let input = quote! {
        #[template(src = #src)]
        struct __YFormat__;
    };

    let i = &syn::parse2(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            resolve_to_self: false,
            is_text: true,
            parent: PARENT,
        }
    )
}

#[proc_macro]
#[cfg(feature = "bytes-buf")]
/// Write handlebars template to `buf-min::Buffer` in this scope with html escape functionality
pub fn ywrite_html(i: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let ParseWriteArg { buf, src, .. } = match syn::parse(i) {
        Ok(arg) => arg,
        Err(e) => return e.to_compile_error().into(),
    };
    // Check correct syn::Expr
    {
        use syn::Expr::*;
        match &buf {
            Field(_) | Path(_) => (),
            _ => {
                return syn::Error::new(buf.span(), "first argument should be a ident or field")
                    .to_compile_error()
                    .into()
            }
        }
    }

    let get_codegen = |_| {
        Box::new(yarte_codegen::WriteBCodeGen::new(
            yarte_codegen::HTMLBytesCodeGen::new(&buf),
            PARENT,
        ))
    };

    let input = quote! {
        #[template(src = #src)]
        struct __YFormat__;
    };

    let i = &syn::parse2(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            resolve_to_self: false,
            parent: PARENT,
            ..Default::default()
        }
    )
}

#[proc_macro]
#[cfg(all(feature = "html-min", feature = "fixed"))]
/// Write handlebars template to `buf-min::Buffer` in this scope without html escape functionality
pub fn ywrite_min(i: TokenStream) -> TokenStream {
    const PARENT: &str = "yarte";

    let ParseWriteArg { buf, src, .. } = match syn::parse(i) {
        Ok(arg) => arg,
        Err(e) => return e.to_compile_error().into(),
    };
    // Check correct syn::Expr
    {
        use syn::Expr::*;
        match &buf {
            Field(_) | Path(_) => (),
            _ => {
                return syn::Error::new(buf.span(), "first argument should be a ident or field")
                    .to_compile_error()
                    .into()
            }
        }
    }

    let get_codegen = |_| {
        Box::new(yarte_codegen::WriteBCodeGen::new(
            yarte_codegen::HTMLMinBytesCodeGen::new(&buf),
            PARENT,
        ))
    };

    let input = quote! {
        #[template(src = #src)]
        struct __YFormat__;
    };

    let i = &syn::parse2(input).unwrap();
    build!(
        i,
        get_codegen,
        HIROptions {
            resolve_to_self: false,
            parent: PARENT,
            ..Default::default()
        }
    )
}

struct ParseWriteArg {
    buf: syn::Expr,
    _comma: syn::Token![,],
    src: syn::LitStr,
    _end_comma: Option<syn::Token![,]>,
}

impl syn::parse::Parse for ParseWriteArg {
    fn parse(input: &ParseBuffer) -> syn::parse::Result<Self> {
        Ok(ParseWriteArg {
            buf: input.parse()?,
            _comma: input.parse()?,
            src: input.parse()?,
            _end_comma: input.parse()?,
        })
    }
}

fn sources_to_tokens<'a>(
    sources: Sources,
    config: &Config,
    s: &'a Struct<'a>,
    mut codegen: Box<dyn CodeGen + 'a>,
    opt: HIROptions,
) -> proc_macro2::TokenStream {
    let mut parsed = BTreeMap::new();
    for (p, src) in sources {
        let nodes = match parse(source_map::get_cursor(p, src)) {
            Ok(n) => n,
            Err(e) => emitter(sources, config, iter::once(e)),
        };
        parsed.insert(p, nodes);
    }

    if cfg!(debug_assertions) && config.print_override == PrintConfig::Ast
        || config.print_override == PrintConfig::All
        || s.print == Print::Ast
        || s.print == Print::All
    {
        eprintln!("{:?}\n", parsed);
    }

    let hir = generate(config, s, &parsed, opt)
        .unwrap_or_else(|e| emitter(sources, config, e.into_iter()));
    // when multiple templates
    source_map::clean();

    let tokens = codegen.gen(hir);

    if cfg!(debug_assertions) && config.print_override == PrintConfig::Code
        || config.print_override == PrintConfig::All
        || s.print == Print::Code
        || s.print == Print::All
    {
        log(
            &tokens.to_string(),
            s.path.to_str().unwrap().to_owned(),
            &config.debug,
        );
    }

    tokens
}

fn read(path: PathBuf, src: String, config: &Config) -> BTreeMap<PathBuf, String> {
    let mut stack = vec![(path, src)];
    let mut visited = BTreeMap::new();

    while let Some((path, src)) = stack.pop() {
        let partials = parse_partials(&src);

        let partials = match partials {
            Ok(n) => n
                .iter()
                .map(|Partial(_, partial, _)| config.resolve_partial(&path, partial.t()))
                .collect::<BTreeSet<_>>(),
            Err(e) => {
                visited.insert(path, src);
                emitter(&visited, config, iter::once(e))
            }
        };
        visited.insert(path, src);

        for partial in partials {
            if !visited.contains_key(&partial) {
                let src = get_source(partial.as_path());
                stack.push((partial, src));
            }
        }
    }

    visited
}
