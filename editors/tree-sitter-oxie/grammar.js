/**
 * Tree-sitter grammar for Oxie.
 *
 * Made for editors: highlighting, outline, indentation, bracket matching and runnables.
 * Declarations (functions, classes, enums, interfaces, tests, ...), calls, member accesses and
 * strings with interpolation are recognized; everything else is kept as a token stream grouped
 * by brackets. That keeps the parser small, fast and tolerant of incomplete code.
 * The Oxie compiler has its own complete parser.
 */

// Words that are always keywords inside code. Declaration keywords (fun, class, val, ...) are
// matched by their declaration rules instead, so they never compete with identifiers.
const KEYWORDS = [
  'if', 'else', 'when', 'match', 'for', 'while', 'do', 'loop', 'return', 'break', 'continue', 'throw',
  'try', 'catch', 'in', 'is', 'as', 'where', 'await', 'move', 'dyn', 'mut', 'ref', 'from', 'to', 'until',
  'downTo', 'step', 'super', 'crate', 'extern', 'out', 'reified', 'throws', 'type', 'async', 'unsafe',
  'static', 'const', 'impl',
];

const MODIFIERS = [
  'pub', 'private', 'internal', 'public', 'override', 'operator', 'static', 'async', 'suspend', 'unsafe',
  'const', 'inline', 'open', 'abstract', 'mut',
];

const kw2 = (a, b) => alias(token(seq(a, /\s+/, b)), `${a} ${b}`);

module.exports = grammar({
  name: 'oxie',

  extras: ($) => [/\s/, $.line_comment, $.block_comment, $.doc_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$.modifier, $.keyword],
    [$._atom, $.parameter_name],
    [$._atom, $.property_name],
    [$._element, $.enum_entry],
    [$._element, $.class_parameter],
    [$.parenthesized, $.class_parameters],
    [$.function_declaration, $.class_declaration, $.class_parameter, $.enum_declaration, $.interface_declaration, $.object_declaration],
    [$.impl_declaration, $.keyword],
    [$.parameter_name, $.variable_declaration],
    [$.property_name, $.variable_declaration],
    [$.class_parameter, $.variable_declaration],
    [$.keyword, $.parameter],
    [$._atom, $.enum_entry],
    [$.interface_declaration],
    [$.enum_declaration],
    [$.class_declaration],
    [$.impl_declaration],
    [$.keyword, $.test_declaration],
  ],

  rules: {
    source_file: ($) => repeat($._element),

    // ------------------------------------------------------------------ comments

    doc_comment: (_) => token(prec(2, seq('///', /[^\n]*/))),
    line_comment: (_) => token(prec(1, seq('//', /[^\n]*/))),
    block_comment: (_) => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),

    // ------------------------------------------------------------------ structure

    _element: ($) =>
      choice(
        $.function_declaration,
        $.class_declaration,
        $.enum_declaration,
        $.interface_declaration,
        $.object_declaration,
        $.impl_declaration,
        $.import_declaration,
        $.variable_declaration,
        $.type_alias,
        $.test_declaration,
        $.annotation,
        $.call_expression,
        $.method_call,
        $.member_access,
        $.macro_invocation,
        $.block,
        $.parenthesized,
        $.bracketed,
        $._atom,
      ),

    _atom: ($) =>
      choice(
        $.identifier,
        $.type_identifier,
        $.constant,
        $.primitive_type,
        $.self,
        $.boolean,
        $.null,
        $.number,
        $.string,
        $.multiline_string,
        $.raw_string,
        $.char_literal,
        $.lifetime,
        $.keyword,
        $.operator,
        ',',
        ';',
        ':',
        '::',
      ),

    keyword: (_) => choice(...KEYWORDS),

    block: ($) => seq('{', repeat($._element), '}'),
    parenthesized: ($) => seq('(', repeat($._element), ')'),
    bracketed: ($) => seq('[', repeat($._element), ']'),

    // ------------------------------------------------------------------ declarations

    modifier: (_) => choice(...MODIFIERS),

    annotation: ($) => prec.right(seq('@', field('name', $.annotation_name), optional($.parenthesized))),
    annotation_name: ($) => prec.right(seq($.identifier, repeat(seq(choice('::', '.'), $.identifier)))),

    function_declaration: ($) =>
      prec.dynamic(
        10,
        prec.right(seq(
          repeat($.modifier),
          choice('fun', 'fn'),
          optional($.type_parameters),
          optional(seq(field('receiver', $._receiver), '.')),
          field('name', $.identifier),
          optional($.type_parameters),
          field('parameters', $.parameters),
          optional(seq(choice(':', '->'), field('return_type', $.return_type))),
          optional(seq('throws', optional($.return_type))),
          optional(field('body', $.block)),
        )),
      ),

    return_type: ($) =>
      prec.right(repeat1(choice($.type_identifier, $.identifier, $.primitive_type, $.constant, $.type_parameters, $.lifetime, $.self, '?', '&', '::', ',', $.parenthesized, $.bracketed, 'mut', 'dyn', 'impl', 'where', ':', '+'))),

    _receiver: ($) => seq(choice($.type_identifier, $.primitive_type), optional($.type_parameters)),

    type_parameters: ($) =>
      prec.dynamic(
        5,
        seq('<', repeat(choice($.type_identifier, $.identifier, $.lifetime, $.primitive_type, ',', ':', '+', '?', '=', '&', $.type_parameters, $.keyword)), '>'),
      ),

    parameters: ($) => seq('(', repeat(choice($.parameter, $._element)), ')'),
    parameter: ($) => prec.dynamic(20, seq(optional(choice('val', 'var', 'mut')), field('name', $.parameter_name), ':')),
    parameter_name: ($) => $.identifier,

    class_declaration: ($) =>
      prec.dynamic(
        10,
        prec.right(
          seq(
            repeat($.modifier),
            choice('class', 'struct', kw2('data', 'class'), kw2('error', 'class'), kw2('sealed', 'class'), kw2('value', 'class')),
            field('name', $.type_identifier),
            optional($.type_parameters),
            optional(field('parameters', $.class_parameters)),
            repeat(choice($.type_identifier, $.primitive_type, ':', ',', '+', '?', '::', '&', $.type_parameters, $.parenthesized, 'where', '=', $.string)),
            optional(field('body', $.class_body)),
          ),
        ),
      ),

    class_parameters: ($) => prec.dynamic(10, seq('(', repeat(choice($.class_parameter, $._element)), ')')),
    class_parameter: ($) => prec.dynamic(20, seq(repeat(choice($.modifier, $.annotation, 'val', 'var')), field('name', $.property_name), ':')),
    property_name: ($) => $.identifier,

    class_body: ($) => prec.dynamic(10, seq('{', repeat(choice($.property_declaration, $.companion_object, $.init_block, $._element)), '}')),
    enum_body: ($) => prec.dynamic(10, seq('{', repeat(choice($.enum_entry, $.property_declaration, $.companion_object, $._element)), '}')),

    property_declaration: ($) => prec.dynamic(20, seq(repeat($.modifier), choice('val', 'var'), field('name', $.property_name))),
    companion_object: ($) => prec.dynamic(10, seq(kw2('companion', 'object'), optional($.identifier), field('body', $.class_body))),
    init_block: ($) => prec.dynamic(10, seq('init', $.block)),

    enum_declaration: ($) =>
      prec.dynamic(
        10,
        seq(
          repeat($.modifier),
          choice('enum', kw2('error', 'enum')),
          field('name', $.type_identifier),
          optional($.type_parameters),
          repeat(choice($.type_identifier, ':', ',', '+', $.type_parameters)),
          field('body', $.enum_body),
        ),
      ),

    enum_entry: ($) => prec.dynamic(10, prec.right(seq(repeat($.annotation), field('name', $.type_identifier), optional(field('parameters', $.class_parameters))))),

    interface_declaration: ($) =>
      prec.dynamic(
        10,
        seq(
          repeat($.modifier),
          choice('interface', 'trait'),
          field('name', $.type_identifier),
          optional($.type_parameters),
          repeat(choice($.type_identifier, ':', ',', '+', $.type_parameters, 'where')),
          field('body', $.class_body),
        ),
      ),

    object_declaration: ($) => prec.dynamic(10, seq(repeat($.modifier), choice('object', 'mod'), field('name', choice($.identifier, $.type_identifier)), field('body', $.class_body))),

    impl_declaration: ($) =>
      prec.dynamic(
        10,
        seq(
          'impl',
          optional($.type_parameters),
          repeat1(choice($.type_identifier, $.identifier, $.primitive_type, 'for', '::', '&', '!', $.type_parameters, 'where', ':', ',', '+')),
          field('body', $.class_body),
        ),
      ),

    import_declaration: ($) =>
      prec.dynamic(
        10,
        prec.right(seq(choice('import', 'use'), repeat1(choice($.identifier, $.type_identifier, $.self, 'crate', 'super', '::', '.', '*', ',', 'as', 'from', $.string, $.import_group)))),
      ),
    import_group: ($) => seq('{', repeat(choice($.identifier, $.type_identifier, $.self, '::', '.', '*', ',', 'as', $.import_group)), '}'),

    variable_declaration: ($) => prec.dynamic(10, prec.right(seq(choice('val', 'var', 'let'), optional('mut'), optional(field('name', choice($.identifier, $.constant)))))),

    type_alias: ($) => prec.dynamic(10, seq('typealias', field('name', $.type_identifier))),

    test_declaration: ($) => prec.dynamic(10, seq(optional('async'), 'test', field('name', $.string), optional('throws'), field('body', $.block))),

    // ------------------------------------------------------------------ expressions

    // `f(x)`, `Point(1, 2)`, and trailing lambdas after lowercase names: `run { }`
    call_expression: ($) =>
      prec.dynamic(5, prec(2, choice(seq(field('function', choice($.identifier, $.type_identifier)), $.parenthesized), seq(field('function', $.identifier), $.block)))),
    method_call: ($) => prec.dynamic(5, prec(3, seq(choice('.', '?.'), field('name', $.identifier), choice($.parenthesized, $.block)))),
    member_access: ($) => prec(1, seq(choice('.', '?.'), field('name', choice($.identifier, $.type_identifier, $.number, $.constant, $.keyword)))),
    macro_invocation: ($) => prec(3, seq(field('macro', $.identifier), token.immediate('!'), choice($.parenthesized, $.bracketed, $.block))),

    // ------------------------------------------------------------------ tokens

    operator: (_) =>
      choice(
        '+', '-', '*', '/', '%', '=', '==', '!=', '<', '>', '<=', '>=', '&&', '||', '!', '&', '|', '^', '<<', '+=', '-=', '*=',
        '/=', '%=', '&=', '|=', '^=', '->', '=>', '?', '?:', '!!', '..', '..=', '..<', '#', '$', '~',
      ),

    identifier: (_) => /(r#)?[a-z_][a-zA-Z0-9_]*/,
    type_identifier: (_) => /[A-Z][a-zA-Z0-9_]*[a-z][a-zA-Z0-9_]*|[A-Z]/,
    constant: (_) => /[A-Z][A-Z0-9_]+/,
    primitive_type: (_) => choice('i8', 'i16', 'i32', 'i64', 'i128', 'isize', 'u8', 'u16', 'u32', 'u64', 'u128', 'usize', 'f32', 'f64', 'bool', 'char', 'str'),
    self: (_) => choice('this', 'self', 'Self'),
    boolean: (_) => choice('true', 'false'),
    null: (_) => 'null',
    lifetime: (_) => /'[a-zA-Z_][a-zA-Z0-9_]*/,

    number: (_) =>
      token(
        choice(
          /0x[0-9a-fA-F_]+([iu](8|16|32|64|128|size))?/,
          /0o[0-7_]+([iu](8|16|32|64|128|size))?/,
          /0b[01_]+([iu](8|16|32|64|128|size))?/,
          /[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9_]+)?([iu](8|16|32|64|128|size)|f32|f64|L)?/,
        ),
      ),

    char_literal: (_) => token(prec(1, seq(optional('b'), "'", choice(/[^'\\\n]/, /\\./, /\\u\{[0-9a-fA-F]+\}/, /\\x[0-9a-fA-F]{2}/), "'"))),

    string: ($) => seq(choice('"', 'b"', 'c"'), repeat(choice($.string_content, $.escape_sequence, $.interpolation, $.dollar)), token.immediate('"')),
    string_content: (_) => token.immediate(prec(1, /[^"\\$]+/)),
    dollar: (_) => token.immediate('$'),
    escape_sequence: (_) => token.immediate(seq('\\', choice(/[^xu\n]/, /u\{[0-9a-fA-F]+\}/, /u[0-9a-fA-F]{4}/, /x[0-9a-fA-F]{2}/, /\n/))),
    interpolation: ($) => choice(seq(token.immediate('${'), repeat($._interp_element), '}'), $.simple_interpolation),
    _interp_element: ($) => choice($.call_expression, $.method_call, $.member_access, $.parenthesized, $.bracketed, $.block, $._atom),
    simple_interpolation: (_) => token.immediate(/\$[a-zA-Z_][a-zA-Z0-9_]*/),

    multiline_string: ($) => seq('"""', repeat(choice($.multiline_content, $.interpolation, $.dollar)), token.immediate('"""')),
    multiline_content: (_) => token.immediate(prec(1, choice(/[^"$]+/, /"[^"$]/, /""[^"$]/))),

    raw_string: (_) =>
      token(
        choice(
          seq(optional(/[bc]/), 'r"', /[^"]*/, '"'),
          seq(optional(/[bc]/), 'r#"', /([^"]|"[^#])*/, '"#'),
          seq(optional(/[bc]/), 'r##"', /([^"]|"[^#]|"#[^#])*/, '"##'),
        ),
      ),
  },
});
