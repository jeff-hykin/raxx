extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token};

/// Parsed input for the shell! macro: a format string followed by optional extra args,
/// and an optional `; ops_expr` for shared options.
struct ShellInput {
    format_str: LitStr,
    extra_args: Vec<Expr>,
    ops: Option<Expr>,
}

impl Parse for ShellInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let format_str: LitStr = input.parse()?;
        let mut extra_args = Vec::new();
        let mut ops = None;
        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            extra_args.push(input.parse()?);
        }
        // Check for ; ops
        if input.peek(Token![;]) {
            let _: Token![;] = input.parse()?;
            ops = Some(input.parse()?);
        }
        Ok(ShellInput {
            format_str,
            extra_args,
            ops,
        })
    }
}

/// A glob call found in the format string, e.g. `{glob("**/*.rs")}`.
struct GlobCall {
    /// Synthetic variable name, e.g. `__raxx_glob_0`.
    var_name: String,
    /// The glob pattern string, e.g. `**/*.rs`.
    pattern: String,
}

/// A flag_if call found in the format string, e.g. `{flag_if("-v", verbose)}`.
struct FlagIfCall {
    /// Synthetic variable name, e.g. `__raxx_flag_0`.
    var_name: String,
    /// The flag string, e.g. `-v`.
    flag: String,
    /// The condition variable name, e.g. `verbose`.
    condition: String,
}

/// Result of pre-processing inline function calls in the format string.
struct InlineCalls {
    /// The format string with function calls replaced by synthetic vars.
    preprocessed: String,
    /// Extracted glob calls.
    globs: Vec<GlobCall>,
    /// Extracted flag_if calls.
    flag_ifs: Vec<FlagIfCall>,
}

/// Pre-process the format string: find `{glob("...")}` and `{flag_if("...", var)}`
/// calls, replace them with synthetic variable placeholders.
fn extract_inline_calls(s: &str) -> InlineCalls {
    let mut result = String::with_capacity(s.len());
    let mut globs = Vec::new();
    let mut flag_ifs = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '{' {
            // Skip escaped `{{`
            if i + 1 < len && chars[i + 1] == '{' {
                result.push_str("{{");
                i += 2;
                continue;
            }
            // Skip `${...}` — shell variable
            if i > 0 && chars[i - 1] == '$' {
                result.push('{');
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
                let trimmed = content.trim();
                if let Some(pattern) = parse_glob_call(trimmed) {
                    let var_name = format!("__raxx_glob_{}", globs.len());
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                    globs.push(GlobCall { var_name, pattern });
                    i = end + 1;
                } else if let Some((flag, condition)) = parse_flag_if_call(trimmed) {
                    let var_name = format!("__raxx_flag_{}", flag_ifs.len());
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                    flag_ifs.push(FlagIfCall {
                        var_name,
                        flag,
                        condition,
                    });
                    i = end + 1;
                } else {
                    // Not a recognized call — pass through unchanged
                    result.push('{');
                    i = start;
                }
            } else {
                result.push('{');
                i = start;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    InlineCalls {
        preprocessed: result,
        globs,
        flag_ifs,
    }
}

/// Try to parse `glob("pattern")` or `glob('pattern')` from a brace-content string.
fn parse_glob_call(s: &str) -> Option<String> {
    let s = s.trim();
    let rest = s.strip_prefix("glob(")?;
    let rest = rest.strip_suffix(')')?;
    let rest = rest.trim();
    // Accept "..." or '...'
    if (rest.starts_with('"') && rest.ends_with('"'))
        || (rest.starts_with('\'') && rest.ends_with('\''))
    {
        Some(rest[1..rest.len() - 1].to_string())
    } else {
        None
    }
}

/// Try to parse `flag_if("flag", condition)` from a brace-content string.
/// Returns `(flag, condition_var_name)` if it matches.
fn parse_flag_if_call(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    let rest = s.strip_prefix("flag_if(")?;
    let rest = rest.strip_suffix(')')?;
    // Split on comma: "flag", condition
    let comma_pos = rest.find(',')?;
    let flag_part = rest[..comma_pos].trim();
    let cond_part = rest[comma_pos + 1..].trim();
    // Flag must be a quoted string
    let flag = if (flag_part.starts_with('"') && flag_part.ends_with('"'))
        || (flag_part.starts_with('\'') && flag_part.ends_with('\''))
    {
        flag_part[1..flag_part.len() - 1].to_string()
    } else {
        return None;
    };
    // Condition must be a valid Rust identifier
    if is_rust_ident(cond_part) {
        Some((flag, cond_part.to_string()))
    } else {
        None
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
            let ops = &shell_input.ops;

            // Pre-process: extract {glob("...")} and {flag_if("...", var)} calls
            let inline = extract_inline_calls(&fmt_value);
            let captures: Vec<String> = find_named_captures(&inline.preprocessed)
                .into_iter()
                .filter(|name| !name.starts_with("__raxx_glob_") && !name.starts_with("__raxx_flag_"))
                .collect();

            let base = if !captures.is_empty() || !inline.globs.is_empty() || !inline.flag_ifs.is_empty() {
                // Interpolation mode (includes inline function calls)
                generate_interpolation_mode(
                    &shell_input.format_str,
                    &inline.preprocessed,
                    &captures,
                    &inline.globs,
                    &inline.flag_ifs,
                    &shell_input.extra_args,
                )
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
            };

            // Wrap with .with_ops() if ops were provided
            if let Some(ops_expr) = ops {
                let base2: TokenStream2 = base.into();
                quote! {
                    (#base2).with_ops(#ops_expr)
                }
                .into()
            } else {
                base
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
///
/// Also handles `{glob("pattern")}` calls — these are expanded at runtime,
/// with errors deferred to command execution time.
fn generate_interpolation_mode(
    fmt_str: &LitStr,
    preprocessed: &str,
    captures: &[String],
    glob_calls: &[GlobCall],
    flag_if_calls: &[FlagIfCall],
    extra_args: &[Expr],
) -> TokenStream {
    // Build the full list of all placeholder names (user captures + glob + flag_if synthetics)
    let all_captures: Vec<String> = captures
        .iter()
        .chain(glob_calls.iter().map(|g| &g.var_name))
        .chain(flag_if_calls.iter().map(|f| &f.var_name))
        .cloned()
        .collect();

    // Transform the format string: escape non-capture braces
    let transformed = transform_format_string(preprocessed, &all_captures);

    // Shadow each user-captured variable with its escaped version
    let shadow_bindings: Vec<TokenStream2> = captures
        .iter()
        .map(|name| {
            let ident = syn::Ident::new(name, fmt_str.span());
            quote! {
                let #ident = ::raxx::EscapeForShell::escape_for_shell(&#ident);
            }
        })
        .collect();

    // Generate glob expansion code: each glob call becomes a match that
    // produces the escaped string + optional deferred error
    let glob_bindings: Vec<TokenStream2> = glob_calls
        .iter()
        .map(|g| {
            let var_ident = syn::Ident::new(&g.var_name, fmt_str.span());
            let err_ident = syn::Ident::new(&format!("{}_err", g.var_name), fmt_str.span());
            let pattern = &g.pattern;
            quote! {
                let (#var_ident, #err_ident) = match ::raxx::glob(#pattern) {
                    Ok(files) => (::raxx::EscapeForShell::escape_for_shell(&files), None),
                    Err(e) => (::std::string::String::new(), Some(::std::format!("{e}"))),
                };
            }
        })
        .collect();

    // Generate flag_if expansion: if condition is true, use escaped flag; else empty string
    let flag_if_bindings: Vec<TokenStream2> = flag_if_calls
        .iter()
        .map(|f| {
            let var_ident = syn::Ident::new(&f.var_name, fmt_str.span());
            let cond_ident = syn::Ident::new(&f.condition, fmt_str.span());
            let flag = &f.flag;
            quote! {
                let #var_ident = if #cond_ident {
                    ::raxx::escape_arg(#flag)
                } else {
                    ::std::string::String::new()
                };
            }
        })
        .collect();

    // Generate deferred error application
    let glob_error_stmts: Vec<TokenStream2> = glob_calls
        .iter()
        .map(|g| {
            let err_ident = syn::Ident::new(&format!("{}_err", g.var_name), fmt_str.span());
            quote! {
                if let Some(__raxx_err) = #err_ident {
                    __raxx_result = __raxx_result._set_deferred_error(__raxx_err);
                }
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

    let has_deferred = !glob_calls.is_empty();

    if !has_deferred {
        // No globs — simple path (no deferred error overhead)
        quote! {
            {
                #(#shadow_bindings)*
                #(#glob_bindings)*
                #(#flag_if_bindings)*
                let mut __raxx_cmd = format!(#transformed);
                #(#append_stmts)*
                ::raxx::Cmd::shell(&__raxx_cmd)
            }
        }
        .into()
    } else {
        quote! {
            {
                #(#shadow_bindings)*
                #(#glob_bindings)*
                #(#flag_if_bindings)*
                let mut __raxx_cmd = format!(#transformed);
                #(#append_stmts)*
                let mut __raxx_result = ::raxx::Cmd::shell(&__raxx_cmd);
                #(#glob_error_stmts)*
                __raxx_result
            }
        }
        .into()
    }
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
