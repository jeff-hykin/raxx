extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token};

/// Parsed input for the shell! macro: a format string followed by optional extra args.
struct ShellInput {
    format_str: LitStr,
    extra_args: Vec<Expr>,
}

impl Parse for ShellInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let format_str: LitStr = input.parse()?;
        let mut extra_args = Vec::new();
        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            extra_args.push(input.parse()?);
        }
        Ok(ShellInput {
            format_str,
            extra_args,
        })
    }
}

/// Check if a string is a valid Rust identifier (for format string named captures).
fn is_rust_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Scan the format string for `{name}` placeholders where `name` is a valid Rust identifier.
/// Returns the list of unique named captures found.
/// Ignores:
/// - `{{` / `}}` (escaped braces)
/// - `${...}` (shell variable syntax — preceded by `$`)
/// - `{content}` where content is not a valid Rust identifier
fn find_named_captures(s: &str) -> Vec<String> {
    let mut names = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '{' {
            // Skip escaped `{{`
            if i + 1 < len && chars[i + 1] == '{' {
                i += 2;
                continue;
            }
            // Skip `${...}` — shell variable
            if i > 0 && chars[i - 1] == '$' {
                i += 1;
                continue;
            }
            // Find closing `}`
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '}' {
                end += 1;
            }
            if end < len {
                let content: String = chars[start..end].iter().collect();
                let content = content.trim();
                // Take just the name part (before any `:` format spec)
                let name = content.split(':').next().unwrap().trim();
                if is_rust_ident(name) {
                    if !names.contains(&name.to_string()) {
                        names.push(name.to_string());
                    }
                }
                i = end + 1;
            } else {
                i += 1;
            }
        } else if chars[i] == '}' && i + 1 < len && chars[i + 1] == '}' {
            i += 2;
        } else {
            i += 1;
        }
    }

    names
}

/// Transform the format string for use with `format!()`:
/// - Keep `{name}` placeholders for valid Rust identifiers (not preceded by `$`)
/// - Escape all other `{` and `}` that aren't part of our placeholders
///   by doubling them (`{` → `{{`, `}` → `}}`)
fn transform_format_string(s: &str, captures: &[String]) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if chars[i] == '{' {
            // Already-escaped `{{`
            if i + 1 < len && chars[i + 1] == '{' {
                result.push_str("{{");
                i += 2;
                continue;
            }
            // Shell `${...}` — escape both braces
            if i > 0 && chars[i - 1] == '$' {
                // Already pushed `$`. Now escape the `{`.
                result.push_str("{{");
                i += 1;
                // Copy until `}`, then escape the `}`
                while i < len && chars[i] != '}' {
                    result.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    result.push_str("}}");
                    i += 1;
                }
                continue;
            }
            // Check if this is one of our captures
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '}' {
                end += 1;
            }
            if end < len {
                let content: String = chars[start..end].iter().collect();
                let trimmed = content.trim();
                let name = trimmed.split(':').next().unwrap().trim();
                if captures.contains(&name.to_string()) {
                    // Keep as format placeholder
                    result.push('{');
                    for j in start..=end {
                        result.push(chars[j]);
                    }
                    i = end + 1;
                } else {
                    // Not our capture — escape
                    result.push_str("{{");
                    i += 1;
                }
            } else {
                result.push_str("{{");
                i += 1;
            }
        } else if chars[i] == '}' {
            // Already-escaped `}}`
            if i + 1 < len && chars[i + 1] == '}' {
                result.push_str("}}");
                i += 2;
                continue;
            }
            // Check if this is closing one of our captures — if so, it was already
            // handled above. Otherwise, escape it.
            // Actually, if we reach a lone `}` here, it means it wasn't handled
            // as part of a capture block above, so escape it.
            result.push_str("}}");
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// The `shell!` proc macro.
///
/// Supports three modes:
///
/// 1. **Interpolation mode** (format string has `{name}` captures):
///    Variables are automatically escaped via `raxx::escape_arg` before interpolation.
///
/// 2. **Append mode** (no placeholders, but extra args given):
///    Extra args are escaped and appended.
///
/// 3. **Plain mode** (no placeholders, no extra args):
///    Passed directly to `Cmd::shell()`.
#[proc_macro]
pub fn shell(input: TokenStream) -> TokenStream {
    let input2: TokenStream2 = input.clone().into();

    // Try to parse as LitStr + extra args
    let parsed: Result<ShellInput, _> = syn::parse(input.clone());

    match parsed {
        Ok(shell_input) => {
            let fmt_value = shell_input.format_str.value();
            let captures = find_named_captures(&fmt_value);

            if !captures.is_empty() {
                // Interpolation mode
                generate_interpolation_mode(&shell_input.format_str, &captures, &shell_input.extra_args)
            } else if !shell_input.extra_args.is_empty() {
                // Append mode
                generate_append_mode(&shell_input.format_str, &shell_input.extra_args)
            } else {
                // Plain mode — pass directly, no format!() needed
                let s = &shell_input.format_str;
                quote! {
                    ::raxx::Cmd::shell(#s)
                }
                .into()
            }
        }
        Err(_) => {
            // Not a string literal — pass through as expression
            quote! {
                {
                    let __raxx_expr = #input2;
                    ::raxx::Cmd::shell(&__raxx_expr)
                }
            }
            .into()
        }
    }
}

/// Generate code for interpolation mode: `shell!("echo {a} | {b}")`.
fn generate_interpolation_mode(
    fmt_str: &LitStr,
    captures: &[String],
    extra_args: &[Expr],
) -> TokenStream {
    let fmt_value = fmt_str.value();

    // Transform the format string: escape non-capture braces
    let transformed = transform_format_string(&fmt_value, captures);

    // Shadow each captured variable with its escaped version
    let shadow_bindings: Vec<TokenStream2> = captures
        .iter()
        .map(|name| {
            let ident = syn::Ident::new(name, fmt_str.span());
            quote! {
                let #ident = ::raxx::EscapeForShell::escape_for_shell(&#ident);
            }
        })
        .collect();

    // Remaining extra args get appended
    let append_stmts: Vec<TokenStream2> = extra_args
        .iter()
        .map(|expr| {
            quote! {
                ::raxx::_append_shell_args(&mut __raxx_cmd, #expr);
            }
        })
        .collect();

    quote! {
        {
            #(#shadow_bindings)*
            let mut __raxx_cmd = format!(#transformed);
            #(#append_stmts)*
            ::raxx::Cmd::shell(&__raxx_cmd)
        }
    }
    .into()
}

/// Generate code for append mode: `shell!("grep", pattern, "file.txt")`.
fn generate_append_mode(fmt_str: &LitStr, extra_args: &[Expr]) -> TokenStream {
    let append_stmts: Vec<TokenStream2> = extra_args
        .iter()
        .map(|expr| {
            quote! {
                ::raxx::_append_shell_args(&mut __raxx_cmd, #expr);
            }
        })
        .collect();

    quote! {
        {
            let mut __raxx_cmd = ::std::string::String::from(#fmt_str);
            #(#append_stmts)*
            ::raxx::Cmd::shell(&__raxx_cmd)
        }
    }
    .into()
}
