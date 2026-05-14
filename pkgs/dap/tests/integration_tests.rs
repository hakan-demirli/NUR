use dap::{apply_patch, parse, run_preflight_checks};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_udiff_integration_simple() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("mathweb/flask/app.py");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        &file_path,
        "class MathWeb:\n    def __init__(self):\n        pass",
    )
    .unwrap();

    let udiff_content = r#"--- mathweb/flask/app.py
+++ mathweb/flask/app.py
@@ -1,3 +1,4 @@
-class MathWeb:
+import sympy
+
+class MathWeb:
     def __init__(self):
"#;

    let mut patches = parse(udiff_content);
    assert_eq!(patches.len(), 1);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_ok());

    let result = apply_patch(&patches[0], false);
    assert!(result.is_ok());

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("import sympy"));
    assert!(content.contains("class MathWeb:"));
    assert!(content.contains("def __init__(self):"));
}

#[test]
fn test_udiff_integration_multiple_hunks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("src/main.rs");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        &file_path,
        "fn main() {\n    println!(\"World\");\n}\n\nfn helper() {\n    // TODO\n}",
    )
    .unwrap();

    let udiff_content = r#"--- src/main.rs
+++ src/main.rs
@@ -1,2 +1,3 @@
 fn main() {
+    println!("Hello");
     println!("World");
@@ -5,2 +6,3 @@
 fn helper() {
-    // TODO
+    // Implementation
+    println!("Helper function");
"#;

    let mut patches = parse(udiff_content);
    assert_eq!(patches.len(), 1);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_ok());

    let result = apply_patch(&patches[0], false);
    assert!(result.is_ok());

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("println!(\"Hello\")"));
    assert!(content.contains("println!(\"World\")"));
    assert!(content.contains("// Implementation"));
    assert!(content.contains("println!(\"Helper function\")"));
    assert!(!content.contains("// TODO"));
}

#[test]
fn test_udiff_integration_new_file() {
    let dir = tempdir().unwrap();
    let _file_path = dir.path().join("new_file.rs");

    let udiff_content = r#"--- /dev/null
+++ new_file.rs
@@ -0,0 +1,3 @@
+fn main() {
+    println!("Hello, world!");
+}
"#;

    let mut patches = parse(udiff_content);
    assert_eq!(patches.len(), 1);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_err());
}

#[test]
fn test_udiff_integration_mixed_formats() {
    let dir = tempdir().unwrap();
    let file1_path = dir.path().join("file1.py");
    let file2_path = dir.path().join("file2.rs");
    let file3_path = dir.path().join("file3.txt");

    fs::write(&file1_path, "print(\"hello\")").unwrap();
    fs::write(&file2_path, "old code").unwrap();
    fs::write(&file3_path, "to be deleted").unwrap();

    let mixed_content = r#"--- file1.py
+++ file1.py
@@ -1,1 +1,2 @@
 print("hello")
+print("world")

file2.rs
<<<<<<< SEARCH
old code
=======
new code
>>>>>>> REPLACE

file3.txt <<<<<<< DELETE
"#;

    let mut patches = parse(mixed_content);
    assert_eq!(patches.len(), 3);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_ok());

    for patch in &patches {
        let result = apply_patch(patch, false);
        assert!(result.is_ok());
    }

    let content1 = fs::read_to_string(&file1_path).unwrap();
    assert!(content1.contains("print(\"hello\")"));
    assert!(content1.contains("print(\"world\")"));

    let content2 = fs::read_to_string(&file2_path).unwrap();
    assert!(content2.contains("new code"));
    assert!(!content2.contains("old code"));

    assert!(!file3_path.exists());
}

#[test]
fn test_udiff_integration_dry_run() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.py");
    let original_content = "def hello():\n    pass";
    fs::write(&file_path, original_content).unwrap();

    let udiff_content = r#"--- test.py
+++ test.py
@@ -1,2 +1,3 @@
 def hello():
+    print("Hello")
     pass
"#;

    let mut patches = parse(udiff_content);
    assert_eq!(patches.len(), 1);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_ok());

    let result = apply_patch(&patches[0], true);
    assert!(result.is_ok());
    assert!(result.unwrap().contains("DRY RUN"));

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, original_content);
}

#[test]
fn test_udiff_integration_context_preservation() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("complex.py");
    let original_content = r#"def function1():
    print("Function 1")
    return 1

def function2():
    print("Function 2")
    return 2

def function3():
    print("Function 3")
    return 3
"#;
    fs::write(&file_path, original_content).unwrap();

    let udiff_content = r#"--- complex.py
+++ complex.py
@@ -4,7 +4,8 @@
 
 def function2():
     print("Function 2")
-    return 2
+    result = 2
+    return result
 
 def function3():
"#;

    let mut patches = parse(udiff_content);
    assert_eq!(patches.len(), 1);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
    }

    let result = run_preflight_checks(&patches);
    assert!(result.is_ok());

    let result = apply_patch(&patches[0], false);
    assert!(result.is_ok());

    let content = fs::read_to_string(&file_path).unwrap();

    assert!(content.contains("result = 2"));
    assert!(content.contains("return result"));
    assert!(!content.contains("return 2"));

    assert!(content.contains("def function1():"));
    assert!(content.contains("def function2():"));
    assert!(content.contains("def function3():"));
    assert!(content.contains("print(\"Function 1\")"));
    assert!(content.contains("print(\"Function 2\")"));
    assert!(content.contains("print(\"Function 3\")"));
}

#[test]
fn test_udiff_concise_operations() {
    let dir = tempdir().unwrap();
    let file_del = dir.path().join("to_delete.txt");
    let file_move = dir.path().join("old_loc.txt");
    let file_move_dest = dir.path().join("new_loc.txt");
    let file_create_dest = dir.path().join("created.txt");

    fs::write(&file_del, "bye").unwrap();
    fs::write(&file_move, "moving content").unwrap();

    let concise_patch = r#"
--- to_delete.txt
+++ /dev/null

--- old_loc.txt
+++ new_loc.txt

--- /dev/null
+++ created.txt
@@ -0,0 +1,1 @@
+fresh content
"#;

    let mut patches = parse(concise_patch);
    assert_eq!(patches.len(), 3);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
        if let dap::types::PatchOp::Move(ref mut dest) = patch.op {
            *dest = dir.path().join(dest);
        }
    }

    let result = run_preflight_checks(&patches);
    assert!(
        result.is_ok(),
        "Preflight checks failed: {:?}",
        result.err()
    );

    for patch in &patches {
        apply_patch(patch, false).unwrap();
    }

    assert!(!file_del.exists());
    assert!(!file_move.exists());
    assert!(file_move_dest.exists());
    assert_eq!(
        fs::read_to_string(file_move_dest).unwrap(),
        "moving content"
    );

    assert!(file_create_dest.exists());
    assert!(fs::read_to_string(file_create_dest)
        .unwrap()
        .contains("fresh content"));
}

#[test]
fn test_udiff_move_and_modify() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("foo.py");
    let dst = dir.path().join("bar.py");

    fs::write(&src, "print('old')").unwrap();

    let patch_content = r#"--- foo.py
+++ bar.py
@@ -1,1 +1,1 @@
-print('old')
+print('new')
"#;

    let mut patches = parse(patch_content);
    assert_eq!(patches.len(), 2);

    for patch in &mut patches {
        patch.file_path = dir.path().join(&patch.file_path);
        if let dap::types::PatchOp::Move(ref mut dest) = patch.op {
            *dest = dir.path().join(dest);
        }
    }

    run_preflight_checks(&patches).unwrap();
    for patch in &patches {
        apply_patch(patch, false).unwrap();
    }

    assert!(!src.exists());
    assert!(dst.exists());
    assert_eq!(fs::read_to_string(dst).unwrap(), "print('new')");
}
