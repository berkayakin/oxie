; `fun main` runs the program
((function_declaration
  name: (identifier) @_name) @run
  (#eq? @_name "main")
  (#set! tag brs-main))

; `test "name" { }` runs one test
((test_declaration
  name: (string (string_content) @BRS_TEST_NAME)) @run
  (#set! tag brs-test))
