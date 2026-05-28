//! 
//! MDMT | Calculation-Graph macros
//! 
//! Автоматизация графа зависимостей для вычислительных шагов.
//! Макрос анализирует тело `impl Eval` и собирает все типы данных, 
//! к которым идет обращение через `ctx.read()` или `ctx.write()`.
//!
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, spanned::Spanned, visit::Visit, visit_mut::VisitMut,
    Error, Expr, ExprCall, ExprMethodCall, ItemImpl, Type
};
use std::collections::HashSet;

///
/// Ищет первое упоминание контекста, чтобы понять, как называется переменная (обычно `ctx`)
#[derive(Default)]
struct CtxFinder {
    ctx_ident: Option<String>,
}
//
impl<'ast> Visit<'ast> for CtxFinder {
    //
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if self.ctx_ident.is_some() { return; }
        let method = node.method.to_string();
        // Реагируем на любой вызов методов доступа к данным
        if method == "read" || method == "read_ref" || method == "write" {
            if let Expr::Path(expr_path) = &*node.receiver {
                if let Some(ident) = expr_path.path.get_ident() {
                    self.ctx_ident = Some(ident.to_string());
                }
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
    //
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if self.ctx_ident.is_some() { return; }
        if let Expr::Path(expr_path) = &*node.func {
            let path_str = quote!(#expr_path).to_string().replace(" ", "");
            // Ищем UFCS вызовы типа ContextRead::<Type>::read(&ctx)
            if path_str.contains("ContextRead") || path_str.contains("ContextWrite") {
                if let Some(arg) = node.args.first() {
                    let mut target_expr = arg;
                    if let Expr::Reference(expr_ref) = arg { target_expr = &*expr_ref.expr; }
                    if let Expr::Path(arg_path) = target_expr {
                        if let Some(ident) = arg_path.path.get_ident() {
                            self.ctx_ident = Some(ident.to_string());
                        }
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}
///
/// Собирает зависимости, обходя все методы внутри блока `impl`
struct EvalVisitor {
    ctx_ident: String,
    reads: HashSet<String>, // Используем HashSet для автоматической дедупликации
    writes: HashSet<String>,
    errors: Vec<Error>,
}
//
impl EvalVisitor {
    fn new(ctx_ident: String) -> Self {
        Self {
            ctx_ident,
            reads: HashSet::new(),
            writes: HashSet::new(),
            errors: Vec::new(),
        }
    }
    ///
    /// Вычищает тип от ссылок и мутабельности для формирования пути к трейту
    fn clean_type(tokens: impl ToTokens) -> String {
        quote!(#tokens).to_string()
            .replace(" ", "")
            .replace("&", "")
            .replace("mut", "")
    }
}
//
impl VisitMut for EvalVisitor {
    ///
    /// Анализирует типизированные переменные: `let x: &InitialCtx = ctx.read_ref();`
    fn visit_local_mut(&mut self, node: &mut syn::Local) {
        if let syn::Pat::Type(pat_type) = &node.pat {
            if let Some(init) = &mut node.init {
                let clean_type = Self::clean_type(&pat_type.ty);
                // Проверяем, является ли правая часть вызовом контекста
                let mut is_write = false;
                let mut matches = false;
                if let Expr::MethodCall(m) = &*init.expr {
                    if let Expr::Path(p) = &*m.receiver {
                        if p.path.is_ident(&self.ctx_ident) {
                            matches = true;
                            is_write = m.method == "write";
                        }
                    }
                }
                if matches {
                    if is_write { self.writes.insert(clean_type); }
                    else { self.reads.insert(clean_type); }
                    return; // Не идем глубже, чтобы не сработал валидатор turbofish
                }
            }
        }
        syn::visit_mut::visit_local_mut(self, node);
    }
    ///
    /// Анализирует вызовы через точку: `ctx.read::<Type>()`
    /// Перехватываем вызовы методов: self.fake_pass_ref(ctx) или self.fake_pass_ref(&mut ctx)
    fn visit_expr_method_call_mut(&mut self, node: &mut ExprMethodCall) {
        // Сначала проверяем, не вызывается ли метод у самого контекста (ctx.read())
        if let Expr::Path(expr_path) = &*node.receiver {
            if expr_path.path.is_ident(&self.ctx_ident) {
                let method = node.method.to_string();
                if method == "read" || method == "read_ref" || method == "write" {
                    if let Some(turbofish) = &node.turbofish {
                        if let Some(arg) = turbofish.args.first() {
                            let clean_type = Self::clean_type(arg);
                            if method == "write" { self.writes.insert(clean_type); }
                            else { self.reads.insert(clean_type); }
                        }
                    } else {
                        self.errors.push(Error::new(node.span(), "Укажите тип явно: ctx.read::<Type>() или let x: Type = ..."));
                    }
                    return; // Вызов к самому контексту - это ок, идем дальше
                }
            }
        }
        // Если метод вызывается у другого объекта (например, self), проверяем его аргументы
        for arg in &node.args {
            let mut target_expr = arg;
            if let Expr::Reference(expr_ref) = arg {
                target_expr = &*expr_ref.expr; // Снимаем & или &mut
            }
            if let Expr::Path(arg_path) = target_expr {
                if arg_path.path.is_ident(&self.ctx_ident) {
                    self.errors.push(Error::new(
                        arg.span(),
                        "Архитектурное ограничение: Запрещено передавать ContextTransaction во вспомогательные методы. Извлеките нужные данные в `eval` и передайте их."
                    ));
                }
            }
        }
        syn::visit_mut::visit_expr_method_call_mut(self, node);
    }
    ///
    /// Анализирует UFCS вызовы: `ContextRead::<Type>::read(&ctx)`
    /// Перехватываем вызовы функций: my_function(ctx) или my_function(&mut ctx)
    fn visit_expr_call_mut(&mut self, node: &mut ExprCall) {
        if let Expr::Path(expr_path) = &*node.func {
            let path_str = quote!(#expr_path).to_string().replace(" ", "");
            let is_read = path_str.contains("ContextRead");
            let is_write = path_str.contains("ContextWrite");
            // Если это легальный UFCS вызов чтения/записи - парсим как раньше
            if is_read || is_write {
                let mut clean_type = None;
                for segment in &expr_path.path.segments {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(arg) = args.args.first() {
                            clean_type = Some(Self::clean_type(arg));
                        }
                    }
                }
                if let Some(ty) = clean_type {
                    if path_str.contains("Write") { self.writes.insert(ty); }
                    else { self.reads.insert(ty); }
                }
                return; // Важно: прерываем обход, чтобы не ругаться на этот вызов
            }
        }
        // Если это ЛЮБОЙ ДРУГОЙ вызов функции, проверяем аргументы
        for arg in &node.args {
            let mut target_expr = arg;
            if let Expr::Reference(expr_ref) = arg {
                target_expr = &*expr_ref.expr; // Снимаем & или &mut
            }
            if let Expr::Path(arg_path) = target_expr {
                if arg_path.path.is_ident(&self.ctx_ident) {
                    self.errors.push(Error::new(
                        arg.span(),
                        "Архитектурное ограничение: Запрещено передавать ContextTransaction во вспомогательные методы. Читайте/пишите данные внутри `eval` и передавайте конкретные значения."
                    ));
                }
            }
        }
        syn::visit_mut::visit_expr_call_mut(self, node);
    }
}
///
/// ### Атрибутный макрос для автоматической реализации трейта `EvalTags`.
/// Генерирует списки IEC-ключей на основе использования контекста (`ContextTransaction`) в коде.
/// 
/// - Читайте / пишите в контекст внутри метода `eval`
/// - ✅ Разрешенные способы доступа к контексту (`ContextTransaction`)
/// ```ignore
/// let initial = ContextRead::<InitialCtx>::read(&ctx);
/// let initial: InitialCtx = ctx.read();
/// let initial: &InitialCtx = initial;
/// ContextWrite::<UnitAreaCtx>::write(ctx, result)
/// ```
/// - ❌ Не используйте неявные типы
/// ```ignore
/// let initial = ctx.read_ref();
/// let initial = ctx.read();
/// ctx.write(result)
/// ```
/// - ❌ Не передавайте контекст во вспомогательные методы, передавайте извлеченные элементы
/// ```ignore
/// Self::fake_pass_ref(&ctx);
/// let ctx = Self::fake_pass(ctx);
/// ```
#[proc_macro_attribute]
pub fn eval_depend(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(item as ItemImpl);
    let self_ty = ast.self_ty.clone();
    // Шаг 1: Находим имя переменной контекста
    let mut finder = CtxFinder::default();
    finder.visit_item_impl(&ast);
    let ctx_ident = finder.ctx_ident.unwrap_or_else(|| "ctx".to_string());
    // Шаг 2: Собираем зависимости, проходя по ВСЕМ методам в impl (включая вспомогательные)
    let mut visitor = EvalVisitor::new(ctx_ident);
    visitor.visit_item_impl_mut(&mut ast);
    if !visitor.errors.is_empty() {
        let compile_errors = visitor.errors.iter().map(Error::to_compile_error);
        return quote! { #(#compile_errors)* #ast }.into();
    }
    // Превращаем имена типов в токены путей для вызова IecId::iec_id()
    let read_tags = visitor.reads.iter().map(|s| {
        let ty: Type = syn::parse_str(s).unwrap();
        quote! { <#ty as crate::domain::IecId>::iec_id().into() }
    });
    let write_tags = visitor.writes.iter().map(|s| {
        let ty: Type = syn::parse_str(s).unwrap();
        quote! { <#ty as crate::domain::IecId>::iec_id().into() }
    });
    let expanded = quote! {
        #ast
        impl crate::domain::EvalTags for #self_ty {
            fn tags(&self) -> crate::domain::CalculationTags {
                crate::domain::CalculationTags {
                    inputs: vec![#(#read_tags),*],
                    outputs: vec![#(#write_tags),*],
                }
            }
        }
    };
    TokenStream::from(expanded)
}
