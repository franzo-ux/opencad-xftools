use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::process::Command;

use ocs_plugin_api::host::{
    codec::{
        entities::{Circle, LwPolyline, Point, Text, UnderlayType},
        objects::ObjectType,
        types::{Handle, Vector2, Vector3},
        EntityType,
    },
    BuiltinPlugin, CommandStep, HostApi, InteractiveCommand,
};
use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};
use ocs_plugin_api::ribbon::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};

const COMMAND: &str = "PNUM";
const PDF_REFS_COMMAND: &str = "PDFREFS";
const TEXT_FRAME_COMMAND: &str = "TFRAME";

#[derive(Clone, Copy)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn apply(self, [x, y]: [f64; 2]) -> [f64; 2] {
        [
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        ]
    }

    fn concat(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.c * next.b,
            b: self.b * next.a + self.d * next.b,
            c: self.a * next.c + self.c * next.d,
            d: self.b * next.c + self.d * next.d,
            e: self.a * next.e + self.c * next.f + self.e,
            f: self.b * next.e + self.d * next.f + self.f,
        }
    }
}

#[derive(Clone, Copy)]
struct PdfUnderlay {
    origin: Vector3,
    scale_x: f64,
    scale_y: f64,
    rotation: f64,
}

fn number(value: &lopdf::Object) -> Option<f64> {
    match value {
        lopdf::Object::Integer(value) => Some(*value as f64),
        lopdf::Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

fn blue(operands: &[lopdf::Object]) -> bool {
    matches!(
        (operands.first().and_then(number), operands.get(1).and_then(number), operands.get(2).and_then(number)),
        (Some(red), Some(green), Some(blue)) if red.abs() < 0.01 && green.abs() < 0.01 && blue > 0.99
    )
}

fn unique(points: &mut Vec<[f64; 2]>) {
    let mut result = Vec::new();
    for point in points.drain(..) {
        if !result.iter().any(|other: &[f64; 2]| {
            (other[0] - point[0]).abs() < 0.01 && (other[1] - point[1]).abs() < 0.01
        }) {
            result.push(point);
        }
    }
    *points = result;
}

fn blue_vector_points(path: &str, page: u32) -> Result<Vec<[f64; 2]>, String> {
    use lopdf::{content::Content, Document};

    let document = Document::load(path).map_err(|error| format!("Cannot read PDF: {error}"))?;
    let (_, page_id) = document
        .get_pages()
        .into_iter()
        .nth(page.saturating_sub(1) as usize)
        .ok_or_else(|| format!("PDF page {page} does not exist"))?;
    let content = Content::decode(&document.get_page_content(page_id))
        .map_err(|error| format!("Cannot parse PDF vectors: {error}"))?;
    let mut matrix = Matrix::IDENTITY;
    let mut states = Vec::new();
    let mut stroke_is_blue = false;
    let mut path_points = Vec::new();
    let mut result = Vec::new();

    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => states.push((matrix, stroke_is_blue)),
            "Q" => {
                if let Some((saved_matrix, saved_stroke)) = states.pop() {
                    matrix = saved_matrix;
                    stroke_is_blue = saved_stroke;
                }
            }
            "cm" if operation.operands.len() == 6 => {
                let values: Option<Vec<_>> = operation.operands.iter().map(number).collect();
                if let Some(values) = values {
                    matrix = matrix.concat(Matrix {
                        a: values[0],
                        b: values[1],
                        c: values[2],
                        d: values[3],
                        e: values[4],
                        f: values[5],
                    });
                }
            }
            "RG" => stroke_is_blue = blue(&operation.operands),
            "m" | "l" if operation.operands.len() >= 2 => {
                if let (Some(x), Some(y)) = (
                    number(&operation.operands[0]),
                    number(&operation.operands[1]),
                ) {
                    path_points.push(matrix.apply([x, y]));
                }
            }
            "re" if operation.operands.len() >= 4 => {
                if let (Some(x), Some(y), Some(width), Some(height)) = (
                    number(&operation.operands[0]),
                    number(&operation.operands[1]),
                    number(&operation.operands[2]),
                    number(&operation.operands[3]),
                ) {
                    for point in [
                        [x, y],
                        [x + width, y],
                        [x + width, y + height],
                        [x, y + height],
                    ] {
                        path_points.push(matrix.apply(point));
                    }
                }
            }
            "S" | "s" | "B" | "B*" | "b" | "b*" => {
                if stroke_is_blue {
                    result.append(&mut path_points);
                }
                path_points.clear();
            }
            "n" | "f" | "F" | "f*" => path_points.clear(),
            _ => {}
        }
    }
    unique(&mut result);
    Ok(result)
}

fn add_pdf_reference_points(host: &mut dyn HostApi) -> Result<usize, String> {
    let underlays: Vec<_> = host
        .document()
        .entities()
        .filter_map(|entity| {
            let EntityType::Underlay(underlay) = entity else {
                return None;
            };
            if underlay.underlay_type != UnderlayType::Pdf {
                return None;
            }
            let ObjectType::UnderlayDefinition(definition) =
                host.document().objects.get(&underlay.definition_handle)?
            else {
                return None;
            };
            Some((
                definition.file_path.clone(),
                definition.page_name.trim().parse().unwrap_or(1),
                PdfUnderlay {
                    origin: underlay.insertion_point,
                    scale_x: underlay.x_scale,
                    scale_y: underlay.y_scale,
                    rotation: underlay.rotation,
                },
            ))
        })
        .collect();
    if underlays.is_empty() {
        return Err("Attach a PDF first, then run PDFREFS.".to_string());
    }

    let mut entities = Vec::new();
    for (path, page, underlay) in underlays {
        for [x, y] in blue_vector_points(&path, page)? {
            let x = x / 72.0 * underlay.scale_x;
            let y = y / 72.0 * underlay.scale_y;
            let (cos, sin) = (underlay.rotation.cos(), underlay.rotation.sin());
            let mut point = Point::new();
            point.location = Vector3::new(
                underlay.origin.x + cos * x - sin * y,
                underlay.origin.y + sin * x + cos * y,
                underlay.origin.z,
            );
            entities.push(EntityType::Point(point));
        }
    }
    let count = entities.len();
    host.push_undo(PDF_REFS_COMMAND);
    host.add_entities(entities);
    host.bump_geometry();
    host.set_dirty();
    Ok(count)
}

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.point_numbering",
    name: "xfTools",
    version: env!("CARGO_PKG_VERSION"),
    description: "Place incrementing labels by clicking points in the drawing.",
    api_version: ApiVersion::CURRENT,
    ribbon_order: 60,
    xdata_apps: &[],
    command_prefixes: &[COMMAND, PDF_REFS_COMMAND, TEXT_FRAME_COMMAND],
};

#[derive(Debug, Clone)]
struct Settings {
    next: i64,
    increment: i64,
    prefix: String,
    text_height: f64,
    offset: f64,
    style: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            next: 1,
            increment: 1,
            prefix: String::new(),
            text_height: 2.5,
            offset: 1.25,
            style: "Standard".to_string(),
        }
    }
}

impl Settings {
    fn label(&self) -> String {
        format!("{}{}", self.prefix, self.next)
    }

    fn apply_command(&mut self, command: &str) -> Result<(), &'static str> {
        let args: Vec<_> = command
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case(COMMAND))
            .map(|(_, tail)| tail.split(',').collect())
            .unwrap_or_else(|| command.split_whitespace().skip(1).collect());
        if args.is_empty() {
            return Ok(());
        }
        if args.len() > 3 || args[0].is_empty() {
            return Err("Usage: PNUM:start,increment,prefix (for example PNUM:1,1,P-)");
        }
        self.next = args[0].parse().map_err(|_| "Start must be an integer")?;
        if let Some(increment) = args.get(1) {
            self.increment = increment
                .parse()
                .map_err(|_| "Increment must be an integer")?;
        }
        if let Some(prefix) = args.get(2) {
            self.prefix = prefix.trim_matches('"').to_string();
        }
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn fields(&self) -> [String; 6] {
        [
            self.next.to_string(),
            self.increment.to_string(),
            self.prefix.clone(),
            self.text_height.to_string(),
            self.offset.to_string(),
            self.style.clone(),
        ]
    }

    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    fn from_fields(fields: Vec<String>) -> Result<Self, String> {
        if fields.len() != 6 {
            return Err("The settings window returned incomplete values.".to_string());
        }
        let next = fields[0]
            .trim()
            .parse()
            .map_err(|_| "Start number must be an integer.".to_string())?;
        let increment = fields[1]
            .trim()
            .parse()
            .map_err(|_| "Increment must be an integer.".to_string())?;
        let text_height: f64 = fields[3]
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| "Text height must be a positive number.".to_string())?;
        let offset: f64 = fields[4]
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| "Offset must be a non-negative number.".to_string())?;
        if !text_height.is_finite() || text_height <= 0.0 {
            return Err("Text height must be a positive number.".to_string());
        }
        if !offset.is_finite() || offset < 0.0 {
            return Err("Offset must be a non-negative number.".to_string());
        }
        if fields[5].trim().is_empty() {
            return Err("Text style cannot be empty.".to_string());
        }
        Ok(Self {
            next,
            increment,
            prefix: fields[2].clone(),
            text_height,
            offset,
            style: fields[5].trim().to_string(),
        })
    }
}

#[derive(Clone, Copy)]
enum ConfigurationState {
    Pending,
    Ready,
    Cancelled,
}

#[cfg(target_os = "windows")]
fn show_settings(settings: Settings) -> Result<Option<Settings>, String> {
    let values = settings.fields().join("\t");
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$v = $env:XFTOOLS_VALUES -split "`t", 6
$form = New-Object System.Windows.Forms.Form
$form.Text = 'xfTools - Point numbering'; $form.ClientSize = New-Object System.Drawing.Size(390, 285)
$form.StartPosition = 'CenterScreen'; $form.FormBorderStyle = 'FixedDialog'; $form.MaximizeBox = $false
function Field($label, $value, $y) {
  $l = New-Object System.Windows.Forms.Label; $l.Text = $label; $l.Location = New-Object System.Drawing.Point(16,$y); $l.Size = New-Object System.Drawing.Size(145,22)
  $t = New-Object System.Windows.Forms.TextBox; $t.Text = $value; $t.Location = New-Object System.Drawing.Point(170,($y-2)); $t.Size = New-Object System.Drawing.Size(200,22)
  $form.Controls.AddRange(@($l,$t)); return $t
}
$start = Field 'Start number' $v[0] 18; $increment = Field 'Increment' $v[1] 52; $prefix = Field 'Prefix' $v[2] 86
$height = Field 'Text height' $v[3] 120; $offset = Field 'Upper-right offset' $v[4] 154; $style = Field 'Text style' $v[5] 188
$cancel = New-Object System.Windows.Forms.Button; $cancel.Text='Cancel'; $cancel.Location=New-Object System.Drawing.Point(210,235); $cancel.DialogResult=[System.Windows.Forms.DialogResult]::Cancel
$ok = New-Object System.Windows.Forms.Button; $ok.Text='Start'; $ok.Location=New-Object System.Drawing.Point(295,235); $ok.DialogResult=[System.Windows.Forms.DialogResult]::OK
$form.AcceptButton=$ok; $form.CancelButton=$cancel; $form.Controls.AddRange(@($cancel,$ok))
if ($form.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { Write-Output ($start.Text + "`t" + $increment.Text + "`t" + $prefix.Text + "`t" + $height.Text + "`t" + $offset.Text + "`t" + $style.Text) }
"#;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("XFTOOLS_VALUES", values)
        .output()
        .map_err(|error| format!("Could not open settings window: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    Settings::from_fields(text.split('\t').map(str::to_owned).collect()).map(Some)
}

#[cfg(target_os = "macos")]
fn show_settings(settings: Settings) -> Result<Option<Settings>, String> {
    let values = settings.fields();
    let script = r#"ObjC.import('Cocoa');

function field(value, y) {
  const input = $.NSTextField.alloc.initWithFrame($.NSMakeRect(160, y, 210, 24));
  input.setStringValue($(value));
  return input;
}

function label(text, y) {
  const output = $.NSTextField.alloc.initWithFrame($.NSMakeRect(0, y, 150, 24));
  output.setStringValue($(text));
  output.setBezeled(false);
  output.setDrawsBackground(false);
  output.setEditable(false);
  output.setSelectable(false);
  return output;
}

function run(argv) {
  const view = $.NSView.alloc.initWithFrame($.NSMakeRect(0, 0, 370, 210));
  const names = ['Start number', 'Increment', 'Prefix', 'Text height', 'Upper-right offset', 'Text style'];
  const inputs = [];
  for (let index = 0; index < names.length; index++) {
    const y = 180 - index * 30;
    const input = field(argv[index], y);
    view.addSubview(label(names[index], y));
    view.addSubview(input);
    inputs.push(input);
  }

  const alert = $.NSAlert.alloc.init;
  alert.messageText = 'xfTools — Point numbering';
  alert.informativeText = 'Configure the labels before selecting points.';
  alert.accessoryView = view;
  alert.addButtonWithTitle('Start');
  alert.addButtonWithTitle('Cancel');
  alert.layout();
  alert.window.makeFirstResponder(inputs[0]);
  if (alert.runModal() != $.NSAlertFirstButtonReturn) return null;

  return inputs.map(input => ObjC.unwrap(input.stringValue)).join('\t');
}"#;
    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .args(values)
        .output()
        .map_err(|error| format!("Could not open settings window: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Settings::from_fields(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .split('\t')
            .map(str::to_owned)
            .collect(),
    )
    .map(Some)
}

#[cfg(target_os = "linux")]
fn show_settings(settings: Settings) -> Result<Option<Settings>, String> {
    // The AppImage does not bundle a portable native form toolkit. Keep Linux
    // functional with its existing inline settings rather than adding a system dependency.
    Ok(Some(settings))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn show_settings(_settings: Settings) -> Result<Option<Settings>, String> {
    Err("This platform has no settings window.".to_string())
}

#[derive(Clone, Copy)]
enum FrameShape {
    Rectangle,
    Circle,
    Slot,
}

#[derive(Clone, Copy)]
enum FrameSize {
    Fit,
    Fixed,
}

#[derive(Clone, Copy)]
struct FrameSettings {
    shape: FrameShape,
    size: FrameSize,
    offset: f64,
    width: f64,
    height: f64,
}

impl Default for FrameSettings {
    fn default() -> Self {
        Self {
            shape: FrameShape::Rectangle,
            size: FrameSize::Fit,
            offset: 1.0,
            width: 10.0,
            height: 5.0,
        }
    }
}

impl FrameSettings {
    fn from_command(command: &str) -> Result<Self, String> {
        let Some((_, values)) = command.split_once(':') else {
            return Ok(Self::default());
        };
        let fields: Vec<String> = values.split(',').map(str::to_owned).collect();
        Self::from_fields(&fields)
    }

    fn from_fields(fields: &[String]) -> Result<Self, String> {
        if fields.len() != 5 {
            return Err("The frame settings are incomplete.".to_string());
        }
        let shape = match fields[0].trim().to_ascii_lowercase().as_str() {
            "rectangle" | "rect" => FrameShape::Rectangle,
            "circle" => FrameShape::Circle,
            "slot" => FrameShape::Slot,
            _ => return Err("Shape must be Rectangle, Circle, or Slot.".to_string()),
        };
        let size = match fields[1].trim().to_ascii_lowercase().as_str() {
            "fit" | "variable" => FrameSize::Fit,
            "fixed" => FrameSize::Fixed,
            _ => return Err("Size mode must be Fit or Fixed.".to_string()),
        };
        let parse = |value: &String, name: &str| {
            value
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .map_err(|_| format!("{name} must be a number."))
        };
        let offset = parse(&fields[2], "Offset")?;
        let width = parse(&fields[3], "Width")?;
        let height = parse(&fields[4], "Height")?;
        if !offset.is_finite()
            || offset < 0.0
            || !width.is_finite()
            || width <= 0.0
            || !height.is_finite()
            || height <= 0.0
        {
            return Err(
                "Offset must be non-negative; width and height must be positive.".to_string(),
            );
        }
        Ok(Self {
            shape,
            size,
            offset,
            width,
            height,
        })
    }
}

#[cfg(target_os = "windows")]
fn show_frame_settings(settings: FrameSettings) -> Result<Option<FrameSettings>, String> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form = New-Object System.Windows.Forms.Form; $form.Text='xfTools - Enclose text'; $form.ClientSize=New-Object System.Drawing.Size(400,260); $form.StartPosition='CenterScreen'; $form.FormBorderStyle='FixedDialog'; $form.MaximizeBox=$false
function Label($text,$y) { $l=New-Object System.Windows.Forms.Label; $l.Text=$text; $l.Location=New-Object System.Drawing.Point(16,$y); $l.Size=New-Object System.Drawing.Size(150,22); $form.Controls.Add($l) }
function TextField($value,$y) { $t=New-Object System.Windows.Forms.TextBox; $t.Text=$value; $t.Location=New-Object System.Drawing.Point(175,$y); $t.Size=New-Object System.Drawing.Size(200,22); $form.Controls.Add($t); return $t }
Label 'Shape' 18; $shape=New-Object System.Windows.Forms.ComboBox; $shape.DropDownStyle='DropDownList'; $shape.Items.AddRange([string[]]@('Rectangle','Circle','Slot')); $shape.SelectedItem=$env:XFTOOLS_FRAME_SHAPE; $shape.Location=New-Object System.Drawing.Point(175,16); $shape.Size=New-Object System.Drawing.Size(200,22); $form.Controls.Add($shape)
Label 'Size mode' 52; $mode=New-Object System.Windows.Forms.ComboBox; $mode.DropDownStyle='DropDownList'; $mode.Items.AddRange([string[]]@('Fit','Fixed')); $mode.SelectedItem=$env:XFTOOLS_FRAME_MODE; $mode.Location=New-Object System.Drawing.Point(175,50); $mode.Size=New-Object System.Drawing.Size(200,22); $form.Controls.Add($mode)
Label 'Offset from text' 86; $offset=TextField $env:XFTOOLS_FRAME_OFFSET 84
Label 'Fixed width' 120; $width=TextField $env:XFTOOLS_FRAME_WIDTH 118
Label 'Fixed height / diameter' 154; $height=TextField $env:XFTOOLS_FRAME_HEIGHT 152
$cancel=New-Object System.Windows.Forms.Button; $cancel.Text='Cancel'; $cancel.Location=New-Object System.Drawing.Point(215,205); $cancel.DialogResult=[System.Windows.Forms.DialogResult]::Cancel
$ok=New-Object System.Windows.Forms.Button; $ok.Text='Apply'; $ok.Location=New-Object System.Drawing.Point(300,205); $ok.DialogResult=[System.Windows.Forms.DialogResult]::OK
$form.AcceptButton=$ok; $form.CancelButton=$cancel; $form.Controls.AddRange(@($cancel,$ok))
if ($form.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { Write-Output ($shape.Text+"`t"+$mode.Text+"`t"+$offset.Text+"`t"+$width.Text+"`t"+$height.Text) }
"#;
    let shape = match settings.shape {
        FrameShape::Rectangle => "Rectangle",
        FrameShape::Circle => "Circle",
        FrameShape::Slot => "Slot",
    };
    let mode = match settings.size {
        FrameSize::Fit => "Fit",
        FrameSize::Fixed => "Fixed",
    };
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("XFTOOLS_FRAME_SHAPE", shape)
        .env("XFTOOLS_FRAME_MODE", mode)
        .env("XFTOOLS_FRAME_OFFSET", settings.offset.to_string())
        .env("XFTOOLS_FRAME_WIDTH", settings.width.to_string())
        .env("XFTOOLS_FRAME_HEIGHT", settings.height.to_string())
        .output()
        .map_err(|error| format!("Could not open frame settings: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let fields: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .trim()
        .split('\t')
        .map(str::to_owned)
        .collect();
    if fields.len() != 5 {
        return Ok(None);
    }
    FrameSettings::from_fields(&fields).map(Some)
}

#[cfg(target_os = "macos")]
fn show_frame_settings(settings: FrameSettings) -> Result<Option<FrameSettings>, String> {
    let values = [
        match settings.shape {
            FrameShape::Rectangle => "Rectangle",
            FrameShape::Circle => "Circle",
            FrameShape::Slot => "Slot",
        }
        .to_string(),
        match settings.size {
            FrameSize::Fit => "Fit",
            FrameSize::Fixed => "Fixed",
        }
        .to_string(),
        settings.offset.to_string(),
        settings.width.to_string(),
        settings.height.to_string(),
    ];
    let script = r#"ObjC.import('Cocoa');
function field(value, y) { const input = $.NSTextField.alloc.initWithFrame($.NSMakeRect(175, y, 200, 24)); input.setStringValue($(value)); return input; }
function label(text, y) { const output = $.NSTextField.alloc.initWithFrame($.NSMakeRect(0, y, 165, 24)); output.setStringValue($(text)); output.setBezeled(false); output.setDrawsBackground(false); output.setEditable(false); return output; }
const view = $.NSView.alloc.initWithFrame($.NSMakeRect(0, 0, 375, 180)); const names = ['Shape (Rectangle/Circle/Slot)', 'Size mode (Fit/Fixed)', 'Offset from text', 'Fixed width / diameter', 'Fixed height']; const inputs = [];
for (let i = 0; i < names.length; i++) { const y = 150 - i * 30; const input = field(arguments[i], y); view.addSubview(label(names[i], y)); view.addSubview(input); inputs.push(input); }
const alert = $.NSAlert.alloc.init; alert.messageText = 'xfTools — Enclose text'; alert.informativeText = 'Configure the frame before selecting TEXT entities.'; alert.accessoryView = view; alert.addButtonWithTitle('Apply'); alert.addButtonWithTitle('Cancel'); alert.layout();
if (alert.runModal() == $.NSAlertFirstButtonReturn) console.log(inputs.map(input => ObjC.unwrap(input.stringValue)).join('\t'));"#;
    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .args(values)
        .output()
        .map_err(|error| format!("Could not open frame settings: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let fields: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .trim()
        .split('\t')
        .map(str::to_owned)
        .collect();
    if fields.len() != 5 {
        return Ok(None);
    }
    FrameSettings::from_fields(&fields).map(Some)
}

#[cfg(target_os = "linux")]
fn show_frame_settings(settings: FrameSettings) -> Result<Option<FrameSettings>, String> {
    // Linux users pass TFRAME:shape,mode,offset,width,height; avoid a GUI dependency.
    Ok(Some(settings))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn show_frame_settings(_settings: FrameSettings) -> Result<Option<FrameSettings>, String> {
    Err("This platform has no frame settings window.".to_string())
}

fn rotate(text: &Text, point: [f64; 2]) -> Vector2 {
    let (cos, sin) = (text.rotation.cos(), text.rotation.sin());
    Vector2::new(
        text.insertion_point.x + cos * point[0] - sin * point[1],
        text.insertion_point.y + sin * point[0] + cos * point[1],
    )
}

fn text_frame(text: &Text, settings: FrameSettings) -> EntityType {
    let text_width = text.value.chars().count() as f64 * text.height * 0.6 * text.width_factor;
    let (width, height) = match settings.size {
        FrameSize::Fit => (
            text_width + 2.0 * settings.offset,
            text.height + 2.0 * settings.offset,
        ),
        FrameSize::Fixed => (settings.width, settings.height),
    };
    let center = [text_width / 2.0, text.height / 2.0];
    match settings.shape {
        FrameShape::Circle => EntityType::Circle(Circle::from_center_radius(
            Vector3::new(
                rotate(text, center).x,
                rotate(text, center).y,
                text.insertion_point.z,
            ),
            width.max(height) / 2.0,
        )),
        FrameShape::Rectangle => {
            let mut frame = LwPolyline::from_points(vec![
                rotate(text, [center[0] - width / 2.0, center[1] - height / 2.0]),
                rotate(text, [center[0] + width / 2.0, center[1] - height / 2.0]),
                rotate(text, [center[0] + width / 2.0, center[1] + height / 2.0]),
                rotate(text, [center[0] - width / 2.0, center[1] + height / 2.0]),
            ]);
            frame.elevation = text.insertion_point.z;
            frame.close();
            EntityType::LwPolyline(frame)
        }
        FrameShape::Slot => {
            let (width, height) = if width >= height {
                (width, height)
            } else {
                (height, width)
            };
            let mut frame = LwPolyline::new();
            let left = center[0] - width / 2.0 + height / 2.0;
            let right = center[0] + width / 2.0 - height / 2.0;
            let bottom = center[1] - height / 2.0;
            let top = center[1] + height / 2.0;
            frame.add_point(rotate(text, [left, bottom]));
            frame.add_point_with_bulge(rotate(text, [right, bottom]), 1.0);
            frame.add_point(rotate(text, [right, top]));
            frame.add_point_with_bulge(rotate(text, [left, top]), 1.0);
            frame.elevation = text.insertion_point.z;
            frame.close();
            EntityType::LwPolyline(frame)
        }
    }
}

struct TextFrameCommand {
    settings: Arc<Mutex<Option<FrameSettings>>>,
    texts: HashMap<Handle, Text>,
}

impl InteractiveCommand for TextFrameCommand {
    fn on_point(&mut self, _point: [f64; 3]) -> CommandStep {
        CommandStep::NeedPoint
    }

    fn prompt(&self) -> String {
        match *self.settings.lock().expect("frame settings lock poisoned") {
            Some(_) => "Select TEXT to enclose (Enter or Esc to finish):".to_string(),
            None => "Configure options in the xfTools window...".to_string(),
        }
    }
    fn needs_object_pick(&self) -> bool {
        true
    }
    fn on_object_pick(&mut self, handle: Handle, _point: [f64; 3]) -> CommandStep {
        let Some(settings) = *self.settings.lock().expect("frame settings lock poisoned") else {
            return CommandStep::NeedPoint;
        };
        match self.texts.get(&handle) {
            Some(text) => CommandStep::Commit(text_frame(text, settings)),
            None => CommandStep::NeedPoint,
        }
    }
    fn on_enter(&mut self) -> CommandStep {
        CommandStep::Done
    }
}

struct PointNumberingModule;
impl CadModule for PointNumberingModule {
    fn id(&self) -> &'static str {
        MANIFEST.id
    }
    fn title(&self) -> &'static str {
        "xfTools"
    }
    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: std::sync::OnceLock<Vec<RibbonGroup>> = std::sync::OnceLock::new();
        GROUPS.get_or_init(|| {
            vec![RibbonGroup {
                title: "Point labels",
                tools: vec![
                    RibbonItem::LargeTool(ToolDef {
                        id: COMMAND,
                        label: "Number points",
                        icon: IconKind::Glyph("1,2…"),
                        event: ModuleEvent::Command(COMMAND.to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: PDF_REFS_COMMAND,
                        label: "PDF reference points",
                        icon: IconKind::Glyph("⌖"),
                        event: ModuleEvent::Command(PDF_REFS_COMMAND.to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: TEXT_FRAME_COMMAND,
                        label: "Enclose text",
                        icon: IconKind::Glyph("▭"),
                        event: ModuleEvent::Command(TEXT_FRAME_COMMAND.to_string()),
                    }),
                ],
            }]
        })
    }
}

struct PointNumberingPlugin {
    settings: Arc<Mutex<Settings>>,
}
impl PointNumberingPlugin {
    fn new() -> Self {
        Self {
            settings: Arc::new(Mutex::new(Settings::default())),
        }
    }
}

impl BuiltinPlugin for PointNumberingPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }
    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(PointNumberingModule)
    }
    fn dispatch(&self, host: &mut dyn HostApi, command: &str) -> bool {
        if command.eq_ignore_ascii_case(TEXT_FRAME_COMMAND)
            || command
                .to_ascii_uppercase()
                .starts_with(&format!("{TEXT_FRAME_COMMAND}:"))
        {
            let initial = match FrameSettings::from_command(command) {
                Ok(settings) => settings,
                Err(error) => {
                    host.push_error(&format!("TFRAME: {error}"));
                    return true;
                }
            };
            let texts = host
                .document()
                .entities()
                .filter_map(|entity| match entity {
                    EntityType::Text(text) => Some((text.common.handle, text.clone())),
                    _ => None,
                })
                .collect();
            let settings = Arc::new(Mutex::new(None));
            let configuration = Arc::clone(&settings);
            std::thread::spawn(move || {
                if let Ok(Some(selected)) = show_frame_settings(initial) {
                    *configuration.lock().expect("frame settings lock poisoned") = Some(selected);
                }
            });
            host.push_info("Set the enclosure options in the xfTools window.");
            host.start_interactive(Box::new(TextFrameCommand { settings, texts }));
            return true;
        }
        if command.eq_ignore_ascii_case(PDF_REFS_COMMAND) {
            match add_pdf_reference_points(host) {
                Ok(count) => host.push_output(&format!(
                    "PDFREFS: created {count} snap reference point(s)."
                )),
                Err(error) => host.push_error(&format!("PDFREFS: {error}")),
            }
            return true;
        }
        if !command.eq_ignore_ascii_case(COMMAND)
            && !command
                .to_ascii_uppercase()
                .starts_with(&format!("{COMMAND}:"))
            && !command
                .to_ascii_uppercase()
                .starts_with(&format!("{COMMAND} "))
        {
            return false;
        }
        let initial = {
            let mut settings = self.settings.lock().expect("settings lock poisoned");
            if let Err(error) = settings.apply_command(command) {
                host.push_error(error);
                return true;
            }
            settings.clone()
        };
        // Dialog is a separate OS process, so dispatch returns before the host timeout.
        let configuration = Arc::new(Mutex::new(ConfigurationState::Pending));
        let settings_for_dialog = Arc::clone(&self.settings);
        let configuration_for_dialog = Arc::clone(&configuration);
        std::thread::spawn(move || {
            let state = match show_settings(initial) {
                Ok(Some(selected)) => {
                    *settings_for_dialog.lock().expect("settings lock poisoned") = selected;
                    ConfigurationState::Ready
                }
                _ => ConfigurationState::Cancelled,
            };
            *configuration_for_dialog
                .lock()
                .expect("configuration lock poisoned") = state;
        });
        host.push_info("Set the numbering options in the xfTools window.");
        host.start_interactive(Box::new(NumberPoints {
            settings: Arc::clone(&self.settings),
            configuration,
        }));
        true
    }
}

struct NumberPoints {
    settings: Arc<Mutex<Settings>>,
    configuration: Arc<Mutex<ConfigurationState>>,
}
impl InteractiveCommand for NumberPoints {
    fn prompt(&self) -> String {
        match *self
            .configuration
            .lock()
            .expect("configuration lock poisoned")
        {
            ConfigurationState::Pending => "Configure options in the xfTools window...".to_string(),
            ConfigurationState::Ready => "Select label point (Enter or Esc to finish):".to_string(),
            ConfigurationState::Cancelled => {
                "Configuration cancelled. Press Enter or Esc.".to_string()
            }
        }
    }
    fn on_point(&mut self, point: [f64; 3]) -> CommandStep {
        match *self
            .configuration
            .lock()
            .expect("configuration lock poisoned")
        {
            ConfigurationState::Pending => return CommandStep::NeedPoint,
            ConfigurationState::Cancelled => return CommandStep::Done,
            ConfigurationState::Ready => {}
        }
        let mut settings = self.settings.lock().expect("settings lock poisoned");
        let label = settings.label();
        settings.next += settings.increment;
        let mut text = Text::with_value(
            label,
            Vector3::new(
                point[0] + settings.offset,
                point[1] + settings.offset,
                point[2],
            ),
        )
        .with_height(settings.text_height);
        text.style = settings.style.clone();
        CommandStep::Commit(EntityType::Text(text))
    }
    fn on_enter(&mut self) -> CommandStep {
        CommandStep::Done
    }
}

ocs_plugin_api::export_plugin!(PointNumberingPlugin::new());

#[cfg(test)]
mod tests {
    use super::{
        blue_vector_points, text_frame, EntityType, FrameSettings, FrameShape, FrameSize, Settings,
        Text, Vector3,
    };
    use lopdf::{
        content::{Content, Operation},
        dictionary, Document, Object, Stream,
    };
    use std::time::{SystemTime, UNIX_EPOCH};
    #[test]
    fn command_sets_numbering_values() {
        let mut settings = Settings::default();
        settings.apply_command("PNUM:10,5,P-").unwrap();
        assert_eq!(settings.label(), "P-10");
        assert_eq!(settings.increment, 5);
    }
    #[test]
    fn dialog_values_set_text_properties() {
        let settings = Settings::from_fields(vec![
            "10".into(),
            "2".into(),
            "P-".into(),
            "3.5".into(),
            "1.0".into(),
            "Notes".into(),
        ])
        .unwrap();
        assert_eq!(settings.text_height, 3.5);
        assert_eq!(settings.style, "Notes");
    }

    #[test]
    fn text_frame_uses_selected_shape_and_size() {
        let text = Text::with_value("AB", Vector3::new(10.0, 20.0, 0.0)).with_height(2.0);
        let fixed = FrameSettings {
            shape: FrameShape::Circle,
            size: FrameSize::Fixed,
            offset: 0.0,
            width: 8.0,
            height: 4.0,
        };
        let EntityType::Circle(circle) = text_frame(&text, fixed) else {
            panic!("expected circle")
        };
        assert_eq!(circle.center, Vector3::new(11.2, 21.0, 0.0));
        assert_eq!(circle.radius, 4.0);
        let slot = FrameSettings {
            shape: FrameShape::Slot,
            size: FrameSize::Fit,
            offset: 1.0,
            width: 1.0,
            height: 1.0,
        };
        let EntityType::LwPolyline(slot) = text_frame(&text, slot) else {
            panic!("expected slot")
        };
        assert!(slot.is_closed);
        assert_eq!(slot.vertices.len(), 4);
    }

    #[test]
    fn extracts_only_blue_vector_endpoints() {
        let mut document = Document::with_version("1.5");
        let pages = document.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("RG", vec![0.into(), 0.into(), 1.into()]),
                Operation::new("m", vec![10.into(), 20.into()]),
                Operation::new("l", vec![30.into(), 20.into()]),
                Operation::new("S", vec![]),
                Operation::new("RG", vec![0.into(), 0.into(), 0.into()]),
                Operation::new("m", vec![40.into(), 50.into()]),
                Operation::new("l", vec![60.into(), 50.into()]),
                Operation::new("S", vec![]),
            ],
        };
        let contents = document.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => contents,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        });
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", catalog);
        let path = std::env::temp_dir().join(format!(
            "xftools-blue-vectors-{}.pdf",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        document.save(&path).unwrap();

        assert_eq!(
            blue_vector_points(path.to_str().unwrap(), 1).unwrap(),
            vec![[10.0, 20.0], [30.0, 20.0]]
        );
        std::fs::remove_file(path).unwrap();
    }
}
