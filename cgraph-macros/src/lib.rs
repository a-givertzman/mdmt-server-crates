//! MDMT | Calculation-Graph macros
//!
//! Формирование IEC-зависимостей вычислительных шагов.
//! Исходное тело impl не изменяется.

use std::collections::{BTreeMap, HashMap};

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input,
    spanned::Spanned,
    visit::Visit,
    Error,
    Expr,
    ExprCall,
    ExprMethodCall,
    GenericArgument,
    Ident,
    ItemImpl,
    Pat,
    Path,
    PathArguments,
    Type,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Read,
    ReadRef,
    Write,
}

impl AccessKind {
    fn method_name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadRef => "read_ref",
            Self::Write => "write",
        }
    }
}

fn method_kind(name: &Ident) -> Option<AccessKind> {
    if name == "read" {
        Some(AccessKind::Read)
    } else if name == "read_ref" {
        Some(AccessKind::ReadRef)
    } else if name == "write" {
        Some(AccessKind::Write)
    } else {
        None
    }
}

fn ungroup_expr(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Paren(node) => &node.expr,
            Expr::Group(node) => &node.expr,
            _ => return expr,
        };
    }
}

fn context_ident(expr: &Expr) -> Option<&Ident> {
    match ungroup_expr(expr) {
        Expr::Reference(node) => context_ident(&node.expr),
        Expr::Path(node) if node.qself.is_none() => node.path.get_ident(),
        _ => None,
    }
}

fn is_context(expr: &Expr, name: &str) -> bool {
    context_ident(expr).is_some_and(|ident| ident == name)
}

/// Проверяет точное имя access-трейта и соответствующего метода.
///
/// Поддерживает:
/// ContextReadRef::<T>::read_ref(&ctx)
/// <ContextTransaction as ContextReadRef<T>>::read_ref(&ctx)
fn ufcs_trait(path: &Path) -> Option<(AccessKind, &syn::PathSegment)> {
    let mut segments = path.segments.iter().rev();
    let method = segments.next()?;
    let trait_segment = segments.next()?;

    let kind = if trait_segment.ident == "ContextRead" {
        AccessKind::Read
    } else if trait_segment.ident == "ContextReadRef" {
        AccessKind::ReadRef
    } else if trait_segment.ident == "ContextWrite" {
        AccessKind::Write
    } else {
        return None;
    };

    if method.ident == kind.method_name() {
        Some((kind, trait_segment))
    } else {
        None
    }
}

fn type_argument(arguments: &PathArguments) -> syn::Result<Option<Type>> {
    match arguments {
        PathArguments::None => Ok(None),

        PathArguments::AngleBracketed(arguments) => {
            if arguments.args.len() != 1 {
                return Err(Error::new_spanned(
                    arguments,
                    "Укажите ровно один тип значения контекста",
                ));
            }

            match arguments.args.first() {
                Some(GenericArgument::Type(ty)) => Ok(Some(ty.clone())),
                _ => Err(Error::new_spanned(
                    arguments,
                    "Ожидается тип значения контекста",
                )),
            }
        }

        _ => Err(Error::new_spanned(
            arguments,
            "Ожидается параметр типа <Type>",
        )),
    }
}

#[derive(Clone, Copy)]
enum Access<'a> {
    Method(&'a ExprMethodCall, AccessKind),
    Ufcs(&'a ExprCall, AccessKind, &'a syn::PathSegment),
}

/// Идентификатор конкретного узла неизменяемого AST.
///
/// Адрес используется только внутри одного обхода для связи вызова
/// с типом переменной. В сгенерированный код он не попадает.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum CallKey {
    Method(usize),
    Ufcs(usize),
}

impl<'a> Access<'a> {
    fn key(self) -> CallKey {
        match self {
            Self::Method(node, _) => {
                CallKey::Method(node as *const ExprMethodCall as usize)
            }
            Self::Ufcs(node, _, _) => {
                CallKey::Ufcs(node as *const ExprCall as usize)
            }
        }
    }

    fn kind(self) -> AccessKind {
        match self {
            Self::Method(_, kind) | Self::Ufcs(_, kind, _) => kind,
        }
    }

    fn explicit_type(self) -> syn::Result<Option<Type>> {
        match self {
            Self::Method(node, _) => match &node.turbofish {
                Some(arguments) => type_argument(
                    &PathArguments::AngleBracketed(arguments.clone()),
                ),
                None => Ok(None),
            },

            Self::Ufcs(_, _, segment) => type_argument(&segment.arguments),
        }
    }

    fn written_value(self) -> Option<&'a Expr> {
        match self {
            Self::Method(node, AccessKind::Write) => node.args.first(),
            Self::Ufcs(node, AccessKind::Write, _) => node.args.iter().nth(1),
            _ => None,
        }
    }

    fn error(self, message: &str) -> Error {
        match self {
            Self::Method(node, _) => Error::new(node.span(), message),
            Self::Ufcs(node, _, _) => Error::new(node.span(), message),
        }
    }
}

fn access<'a>(expr: &'a Expr, ctx: &str) -> Option<Access<'a>> {
    match ungroup_expr(expr) {
        Expr::MethodCall(node) if is_context(&node.receiver, ctx) => {
            method_kind(&node.method).map(|kind| Access::Method(node, kind))
        }

        Expr::Call(node) => {
            let Expr::Path(function) = ungroup_expr(&node.func) else {
                return None;
            };

            let (kind, segment) = ufcs_trait(&function.path)?;

            if node.args.first().is_some_and(|arg| is_context(arg, ctx)) {
                Some(Access::Ufcs(node, kind, segment))
            } else {
                None
            }
        }

        _ => None,
    }
}

/// Находит имя переменной контекста по первому обращению.
#[derive(Default)]
struct CtxFinder {
    ctx_ident: Option<String>,
}

impl<'ast> Visit<'ast> for CtxFinder {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if self.ctx_ident.is_some() {
            return;
        }

        if method_kind(&node.method).is_some() {
            if let Some(ident) = context_ident(&node.receiver) {
                if ident != "self" {
                    self.ctx_ident = Some(ident.to_string());
                }
            }
        }

        if self.ctx_ident.is_none() {
            syn::visit::visit_expr_method_call(self, node);
        }
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if self.ctx_ident.is_some() {
            return;
        }

        if let Expr::Path(function) = ungroup_expr(&node.func) {
            if ufcs_trait(&function.path).is_some() {
                if let Some(ident) = node.args.first().and_then(context_ident) {
                    self.ctx_ident = Some(ident.to_string());
                }
            }
        }

        if self.ctx_ident.is_none() {
            syn::visit::visit_expr_call(self, node);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Option,
    Result,
    Value,
}

#[derive(Clone, Copy)]
struct ReadShape {
    container: Container,
    references: usize,
}

/// Находит исходное чтение внутри цепочки операций над Option/Result.
///
/// Тип конечной переменной используется только для операций
/// с известным преобразованием результата.
fn read_root<'a>(
    expr: &'a Expr,
    ctx: &str,
) -> Option<(Access<'a>, ReadShape)> {
    let expr = ungroup_expr(expr);

    if let Some(call) = access(expr, ctx) {
        if call.kind() == AccessKind::Write {
            return None;
        }

        return Some((
            call,
            ReadShape {
                container: Container::Option,
                references: usize::from(call.kind() == AccessKind::ReadRef),
            },
        ));
    }

    match expr {
        Expr::Try(node) => {
            let (call, mut shape) = read_root(&node.expr, ctx)?;

            if shape.container == Container::Value {
                return None;
            }

            shape.container = Container::Value;
            Some((call, shape))
        }

        Expr::MethodCall(node) => {
            let (call, mut shape) = read_root(&node.receiver, ctx)?;

            match node.method.to_string().as_str() {
                "unwrap"
                | "expect"
                | "unwrap_or"
                | "unwrap_or_else"
                | "unwrap_or_default"
                    if shape.container != Container::Value =>
                {
                    shape.container = Container::Value;
                }

                "ok_or" | "ok_or_else"
                    if shape.container == Container::Option =>
                {
                    shape.container = Container::Result;
                }

                "map_err" | "inspect_err"
                    if shape.container == Container::Result => {}

                "inspect" if shape.container != Container::Value => {}

                "as_ref" if shape.container != Container::Value => {
                    shape.references += 1;
                }

                "cloned" | "copied"
                    if shape.container == Container::Option
                        && shape.references > 0 =>
                {
                    shape.references -= 1;
                }

                _ => return None,
            }

            Some((call, shape))
        }

        _ => None,
    }
}

fn ungroup_type(mut ty: &Type) -> &Type {
    loop {
        ty = match ty {
            Type::Paren(node) => &node.elem,
            Type::Group(node) => &node.elem,
            _ => return ty,
        };
    }
}

/// Снимает только внешнюю оболочку результата чтения.
///
/// Option<T> внутри самого типа данных остаётся нетронутым.
fn container_value<'a>(
    ty: &'a Type,
    container: Container,
) -> syn::Result<&'a Type> {
    let ty = ungroup_type(ty);

    if container == Container::Value {
        return Ok(ty);
    }

    let (name, module, arity) = match container {
        Container::Option => ("Option", "option", 1),
        Container::Result => ("Result", "result", 2),
        Container::Value => return Ok(ty),
    };

    if let Type::Path(path) = ty {
        if path.qself.is_none() {
            let segments: Vec<_> = path.path.segments.iter().collect();

            let standard = match segments.as_slice() {
                [last] => last.ident == name,

                [root, middle, last] => {
                    (root.ident == "std" || root.ident == "core")
                        && middle.ident == module
                        && last.ident == name
                        && matches!(root.arguments, PathArguments::None)
                        && matches!(middle.arguments, PathArguments::None)
                }

                _ => false,
            };

            if standard {
                if let Some(last) = segments.last() {
                    if let PathArguments::AngleBracketed(arguments) =
                        &last.arguments
                    {
                        if arguments.args.len() == arity {
                            if let Some(GenericArgument::Type(value)) =
                                arguments.args.first()
                            {
                                return Ok(ungroup_type(value));
                            }
                        }
                    }
                }
            }
        }
    }

    Err(Error::new_spanned(
        ty,
        format!(
            "Результат чтения имеет оболочку {name}: \
             укажите её в типе переменной либо используйте \
             UFCS с явным типом значения"
        ),
    ))
}

fn dependency_type(
    ty: &Type,
    shape: ReadShape,
) -> syn::Result<Type> {
    let mut value = container_value(ty, shape.container)?;

    for _ in 0..shape.references {
        let Type::Reference(reference) = ungroup_type(value) else {
            return Err(Error::new_spanned(
                value,
                "Для ссылочного чтения ожидается ссылочный тип",
            ));
        };

        value = &reference.elem;
    }

    // Внутренние ссылки, generic-аргументы и имена типов сохраняются.
    Ok(ungroup_type(value).clone())
}

fn binding_type(pattern: &syn::PatIdent, ty: Type) -> Type {
    if pattern.by_ref.is_some() {
        if pattern.mutability.is_some() {
            syn::parse_quote!(&mut #ty)
        } else {
            syn::parse_quote!(&#ty)
        }
    } else {
        ty
    }
}

struct EvalVisitor {
    ctx_ident: String,
    reads: BTreeMap<String, Type>,
    writes: BTreeMap<String, Type>,

    // None означает, что ошибка вывода уже зарегистрирована.
    inferred_reads: HashMap<CallKey, Option<Type>>,

    // None блокирует поиск типа в наружных областях при затенении.
    scopes: Vec<HashMap<String, Option<Type>>>,
    errors: Vec<Error>,
}

impl EvalVisitor {
    fn new(ctx_ident: String) -> Self {
        Self {
            ctx_ident,
            reads: BTreeMap::new(),
            writes: BTreeMap::new(),
            inferred_reads: HashMap::new(),
            scopes: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn record(&mut self, kind: AccessKind, ty: Type) {
        let key = ty.to_token_stream().to_string();

        let dependencies = if kind == AccessKind::Write {
            &mut self.writes
        } else {
            &mut self.reads
        };

        dependencies.entry(key).or_insert(ty);
    }

    fn argument_type(&self, expr: &Expr) -> Option<Type> {
        match ungroup_expr(expr) {
            Expr::Path(node) if node.qself.is_none() => {
                let name = node.path.get_ident()?.to_string();

                for scope in self.scopes.iter().rev() {
                    if let Some(ty) = scope.get(&name) {
                        return ty.clone();
                    }
                }

                None
            }

            Expr::Struct(node) => Some(Type::Path(syn::TypePath {
                qself: node.qself.clone(),
                path: node.path.clone(),
            })),

            Expr::Cast(node) => Some((*node.ty).clone()),

            _ => None,
        }
    }

    fn process_access(&mut self, call: Access<'_>) {
        let explicit = match call.explicit_type() {
            Ok(ty) => ty,
            Err(error) => {
                self.errors.push(error);
                return;
            }
        };

        let ty = if let Some(ty) = explicit {
            Some(ty)
        } else if let Some(inferred) = self.inferred_reads.get(&call.key()) {
            match inferred {
                Some(ty) => Some(ty.clone()),
                None => return,
            }
        } else {
            call.written_value()
                .and_then(|value| self.argument_type(value))
        };

        if let Some(ty) = ty {
            self.record(call.kind(), ty);
            return;
        }

        let message = if call.kind() == AccessKind::Write {
            "Укажите тип записываемого значения: \
             let value: Type = ...; ctx.write(value), \
             либо ContextWrite::<Type>::write(ctx, value)"
        } else {
            "Укажите тип результата чтения в let \
             с учётом Option/Result и ссылок либо используйте \
             ContextRead::<Type>::read(&ctx) / \
             ContextReadRef::<Type>::read_ref(&ctx)"
        };

        self.errors.push(call.error(message));
    }

    fn check_arguments<'a>(
        &mut self,
        arguments: impl Iterator<Item = &'a Expr>,
    ) {
        for argument in arguments {
            if is_context(argument, &self.ctx_ident) {
                self.errors.push(Error::new(
                    argument.span(),
                    "Архитектурное ограничение: запрещено передавать \
                     ContextTransaction во вспомогательные методы. \
                     Извлеките данные в eval и передайте конкретные значения.",
                ));
            }
        }
    }
}

impl<'ast> Visit<'ast> for EvalVisitor {
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let mut parameters = HashMap::new();

        for parameter in &node.sig.inputs {
            if let syn::FnArg::Typed(parameter) = parameter {
                if let Pat::Ident(pattern) = &*parameter.pat {
                    let ty = binding_type(
                        pattern,
                        (*parameter.ty).clone(),
                    );

                    parameters.insert(
                        pattern.ident.to_string(),
                        Some(ty),
                    );
                }
            }
        }

        self.scopes.push(parameters);
        syn::visit::visit_impl_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.scopes.push(HashMap::new());
        syn::visit::visit_block(self, node);
        self.scopes.pop();
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let (Pat::Type(pattern), Some(init)) = (&node.pat, &node.init) {
            if let Some((call, shape)) =
                read_root(&init.expr, &self.ctx_ident)
            {
                // Явный тип UFCS/turbofish не требует вывода из let.
                if matches!(call.explicit_type(), Ok(None)) {
                    match dependency_type(&pattern.ty, shape) {
                        Ok(ty) => {
                            self.inferred_reads.insert(call.key(), Some(ty));
                        }

                        Err(error) => {
                            self.inferred_reads.insert(call.key(), None);
                            self.errors.push(error);
                        }
                    }
                }
            }
        }

        // Не прекращаем обход: внутри аргументов и замыканий
        // могут находиться дополнительные чтения или запрещённая
        // передача контекста.
        syn::visit::visit_local(self, node);

        let (pattern, ty) = match &node.pat {
            Pat::Type(pattern) => (
                &*pattern.pat,
                Some((*pattern.ty).clone()),
            ),
            pattern => (pattern, None),
        };

        if let Pat::Ident(pattern) = pattern {
            let ty = ty.map(|ty| binding_type(pattern, ty));

            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(pattern.ident.to_string(), ty);
            }
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if is_context(&node.receiver, &self.ctx_ident) {
            if let Some(kind) = method_kind(&node.method) {
                self.process_access(Access::Method(node, kind));

                // Анализируем и аргументы операции доступа.
                syn::visit::visit_expr_method_call(self, node);
                return;
            }
        }

        self.check_arguments(node.args.iter());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(function) = ungroup_expr(&node.func) {
            if let Some((kind, segment)) = ufcs_trait(&function.path) {
                if node.args.first().is_some_and(|argument| {
                    is_context(argument, &self.ctx_ident)
                }) {
                    self.process_access(Access::Ufcs(node, kind, segment));

                    // Первый аргумент — разрешённый receiver контекста.
                    // Остальные аргументы проверяются обычным образом.
                    self.check_arguments(node.args.iter().skip(1));

                    syn::visit::visit_expr_call(self, node);
                    return;
                }
            }
        }

        self.check_arguments(node.args.iter());
        syn::visit::visit_expr_call(self, node);
    }
}

/// Формирует EvalTags по обращениям к ContextTransaction.
///
/// Поддерживаемые примеры:
///
/// ```ignore
/// let initial: Option<&InitialCtx> = ctx.read_ref();
///
/// let initial: &InitialCtx = ctx.read_ref().unwrap();
///
/// let initial: &InitialCtx = ctx.read_ref()
///     .ok_or_else(|| error.pass_with("Missing initial context"))?;
///
/// let initial = ContextReadRef::<InitialCtx>::read_ref(&ctx);
///
/// let result: UnitAreaCtx = calculate();
/// ctx.write(result)
///
/// ContextWrite::<UnitAreaCtx>::write(ctx, result)
/// ```
///
/// Для произвольных преобразований результата, например map/and_then,
/// тип зависимости следует задавать через UFCS.
///
/// Анализируются все методы аннотированного impl.
/// Используйте одно имя переменной контекста в пределах impl.
#[proc_macro_attribute]
pub fn eval_depend(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(item as ItemImpl);

    let mut finder = CtxFinder::default();
    finder.visit_item_impl(&ast);

    let ctx_ident = finder.ctx_ident.unwrap_or_else(|| "ctx".to_owned());

    let mut visitor = EvalVisitor::new(ctx_ident);
    visitor.visit_item_impl(&ast);

    if !visitor.errors.is_empty() {
        let errors = visitor.errors.iter().map(Error::to_compile_error);

        return quote! {
            #(#errors)*
            #ast
        }
        .into();
    }

    let self_ty = &ast.self_ty;
    let mut generics = ast.generics.clone();

    // Ограничения добавляются только в генерируемый EvalTags impl.
    let mut dependencies = BTreeMap::new();

    for (key, ty) in visitor.reads.iter().chain(visitor.writes.iter()) {
        dependencies.entry(key.clone()).or_insert(ty);
    }

    for ty in dependencies.values() {
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! {
                #ty: crate::domain::IecId
            });
    }

    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let read_tags = visitor.reads.values().map(|ty| {
        quote! {
            <#ty as crate::domain::IecId>::iec_id().into()
        }
    });

    let write_tags = visitor.writes.values().map(|ty| {
        quote! {
            <#ty as crate::domain::IecId>::iec_id().into()
        }
    });

    quote! {
        #ast

        impl #impl_generics crate::domain::EvalTags
            for #self_ty
            #where_clause
        {
            fn tags(&self) -> crate::domain::CalculationTags {
                crate::domain::CalculationTags {
                    inputs: ::std::vec![#(#read_tags),*],
                    outputs: ::std::vec![#(#write_tags),*],
                }
            }
        }
    }
    .into()
}