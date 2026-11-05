//! Describe en texto una pizarra de Excalidraw (formas, textos, conexiones, marcos e imágenes) para que los agentes puedan leerla.

use serde_json::Value;

/// Describe en texto la escena (JSON de Excalidraw con los elementos en `elements`),
/// ignorando los borrados: marcos, formas, textos sueltos, conexiones, imágenes y trazos.
/// `(pizarra vacía)` si no queda ningún elemento visible.
pub fn describir(escena: &Value) -> String {
    let elems = visibles(escena);
    if elems.is_empty() {
        return "(pizarra vacía)".into();
    }
    let mut secciones: Vec<String> = Vec::new();
    for (titulo, lineas) in [
        ("Marcos:", marcos(&elems)),
        ("Formas:", formas(&elems)),
        ("Textos sueltos:", textos_sueltos(&elems)),
        ("Conexiones:", conexiones(&elems)),
        ("Imágenes:", imagenes(&elems)),
        ("Dibujo a mano alzada:", trazos(&elems)),
    ].into_iter() {
        if !lineas.is_empty() {
            secciones.push(format!("{titulo}\n{}", lineas.join("\n")));
        }
    }
    secciones.join("\n\n")
}

/// Elementos que quedan en la escena tras descartar los borrados (`isDeleted`).
fn visibles(escena: &Value) -> Vec<&Value> {
    escena.get("elements").and_then(Value::as_array).map(|l| l.iter().filter(|e| !borrado(e)).collect()).unwrap_or_default()
}

/// `isDeleted` de un elemento (falta o false: no borrado).
fn borrado(e: &Value) -> bool {
    e.get("isDeleted").and_then(Value::as_bool).unwrap_or(false)
}

/// Tipo del elemento (`rectangle`, `text`, `arrow`…), o vacío si no lo tiene.
fn tipo(e: &Value) -> &str {
    e.get("type").and_then(Value::as_str).unwrap_or_default()
}

/// Id del elemento, o vacío si no lo tiene.
fn id(e: &Value) -> &str {
    e.get("id").and_then(Value::as_str).unwrap_or_default()
}

/// Id de la caja o flecha que contiene un texto (`containerId`), si lo tiene y no es vacío.
fn contenedor(e: &Value) -> Option<&str> {
    e.get("containerId").and_then(Value::as_str).filter(|c| !c.is_empty())
}

/// Etiqueta de un elemento: su texto si es de tipo text, o el texto del text que la señala
/// (`containerId` = su id). Recortada y en una sola línea; vacía si no tiene.
fn etiqueta(e: &Value, elems: &[&Value]) -> String {
    if tipo(e) == "text" {
        return e.get("text").and_then(Value::as_str).map(texto_en_linea).unwrap_or_default();
    }
    let id = id(e);
    elems.iter().find(|t| tipo(t) == "text" && contenedor(t) == Some(id))
        .and_then(|t| t.get("text").and_then(Value::as_str))
        .map(texto_en_linea)
        .unwrap_or_default()
}

/// Recorta espacios y cambia los saltos de línea por un espacio (texto en una línea).
fn texto_en_linea(texto: &str) -> String {
    texto.split(['\r', '\n']).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Nombre de un elemento para mostrar: «etiqueta» si la tiene; si no, su tipo en español
/// entre corchetes (rectangle→[caja], ellipse→[elipse]… cualquier otro→[elemento]).
fn nombre_para_mostrar(e: &Value, elems: &[&Value]) -> String {
    let et = etiqueta(e, elems);
    if !et.is_empty() {
        return format!("«{et}»");
    }
    match tipo(e) {
        "rectangle" => "[caja]".into(),
        "ellipse" => "[elipse]".into(),
        "diamond" => "[rombo]".into(),
        "image" => "[imagen]".into(),
        "frame" => "[marco]".into(),
        _ => "[elemento]".into(),
    }
}

/// Nombre de un marco para mostrar: «name» si lo tiene; si no, [marco].
fn nombre_marco(m: &Value) -> String {
    match m.get("name").and_then(Value::as_str).map(str::trim) {
        Some(n) if !n.is_empty() => format!("«{n}»"),
        _ => "[marco]".into(),
    }
}

/// Marco que contiene un elemento, si su `frameId` señala un marco existente (visible).
fn marco_contenedor<'a>(e: &Value, elems: &[&'a Value]) -> Option<&'a Value> {
    let fid = e.get("frameId").and_then(Value::as_str)?;
    elems.iter().copied().find(|m| tipo(m) == "frame" && id(m) == fid)
}

/// Elemento al que conecta un extremo de una flecha (`startBinding`/`endBinding` con
/// `elementId` de un elemento existente), si lo hay.
fn extremo<'a>(b: Option<&Value>, elems: &[&'a Value]) -> Option<&'a Value> {
    let eid = b?.get("elementId")?.as_str()?;
    elems.iter().copied().find(|e| id(e) == eid)
}

/// Sección «Marcos:», con el nombre de cada marco.
fn marcos(elems: &[&Value]) -> Vec<String> {
    elems.iter().filter(|e| tipo(e) == "frame").map(|m| format!("- {}", nombre_marco(m))).collect()
}

/// Sección «Formas:», con el tipo en español y el nombre de cada rectangle, ellipse o diamond,
/// y «(en «marco»)» si está dentro de un marco existente.
fn formas(elems: &[&Value]) -> Vec<String> {
    elems.iter().filter(|e| matches!(tipo(e), "rectangle" | "ellipse" | "diamond")).map(|e| {
        let tipo_es = match tipo(e) {
            "rectangle" => "caja",
            "ellipse" => "elipse",
            _ => "rombo",
        };
        let mut linea = format!("- {tipo_es} {}", nombre_para_mostrar(e, elems));
        if let Some(m) = marco_contenedor(e, elems) {
            linea.push_str(&format!(" (en {})", nombre_marco(m)));
        }
        linea
    }).collect()
}

/// Sección «Textos sueltos:», con el texto de cada text sin `containerId`.
fn textos_sueltos(elems: &[&Value]) -> Vec<String> {
    elems.iter().filter(|t| tipo(t) == "text" && contenedor(t).is_none()).map(|t| {
        let texto = t.get("text").and_then(Value::as_str).map(texto_en_linea).unwrap_or_default();
        format!("- «{texto}»")
    }).collect()
}

/// Sección «Conexiones:»: origen → destino de cada flecha o línea conectada por sus dos
/// extremos, o «flecha sin conectar del todo»; con «etiqueta» si la tiene.
fn conexiones(elems: &[&Value]) -> Vec<String> {
    elems.iter().filter(|f| matches!(tipo(f), "arrow" | "line")).map(|f| {
        let mut linea = match extremo(f.get("startBinding"), elems).zip(extremo(f.get("endBinding"), elems)) {
            Some((origen, destino)) => format!("- {} → {}", nombre_para_mostrar(origen, elems), nombre_para_mostrar(destino, elems)),
            None => "- flecha sin conectar del todo".into(),
        };
        let et = etiqueta(f, elems);
        if !et.is_empty() {
            linea.push_str(&format!(": «{et}»"));
        }
        linea
    }).collect()
}

/// Sección «Imágenes:», con el número de imágenes y la nota del diagrama Mermaid en el código.
fn imagenes(elems: &[&Value]) -> Vec<String> {
    let n = elems.iter().filter(|e| tipo(e) == "image").count();
    if n == 0 {
        return Vec::new();
    }
    let palabra = if n == 1 { "imagen" } else { "imágenes" };
    vec![format!("- {n} {palabra}: si es un diagrama Mermaid que no se pudo pasar a formas, su contenido está en el código fuente.")]
}

/// Sección «Dibujo a mano alzada:», con el número de trazos freedraw.
fn trazos(elems: &[&Value]) -> Vec<String> {
    let n = elems.iter().filter(|e| tipo(e) == "freedraw").count();
    if n == 0 {
        return Vec::new();
    }
    vec![format!("- {n} trazo{}", if n == 1 { "" } else { "s" })]
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn pizarra_vacia_y_solo_borrados() {
        assert_eq!(describir(&json!({"elements": []})), "(pizarra vacía)");
        assert_eq!(describir(&json!({})), "(pizarra vacía)");
        assert_eq!(describir(&json!({"elements": [{"id": "a", "type": "rectangle", "isDeleted": true}]})), "(pizarra vacía)");
    }

    #[test]
    fn marcos_formas_y_conexion_etiquetada() {
        let out = describir(&json!({"elements": [
            {"id": "m", "type": "frame", "name": "M"},
            {"id": "a", "type": "rectangle", "frameId": "m"},
            {"id": "ta", "type": "text", "containerId": "a", "text": "A"},
            {"id": "b", "type": "rectangle", "frameId": "m"},
            {"id": "tb", "type": "text", "containerId": "b", "text": "B"},
            {"id": "f", "type": "arrow", "startBinding": {"elementId": "a"}, "endBinding": {"elementId": "b"}},
            {"id": "tf", "type": "text", "containerId": "f", "text": "llama"},
        ]}));
        assert!(out.contains("Marcos:\n- «M»"), "…{out}");
        assert!(out.contains("- caja «A» (en «M»)\n- caja «B» (en «M»)"), "…{out}");
        assert!(out.contains("- «A» → «B»: «llama»"), "…{out}");
    }

    #[test]
    fn flecha_a_medio_conectar() {
        let out = describir(&json!({"elements": [
            {"id": "a", "type": "rectangle"},
            {"id": "f", "type": "arrow", "startBinding": {"elementId": "a"}, "endBinding": null},
        ]}));
        assert!(out.contains("Conexiones:\n- flecha sin conectar del todo"), "…{out}");
    }

    #[test]
    fn textos_sueltos_no_etiquetas() {
        let out = describir(&json!({"elements": [
            {"id": "a", "type": "rectangle"},
            {"id": "ta", "type": "text", "containerId": "a", "text": "etiqueta de caja"},
            {"id": "t1", "type": "text", "text": "texto libre\nsegunda línea"},
        ]}));
        assert!(out.contains("Formas:\n- caja «etiqueta de caja»"), "…{out}");
        let sec = out.split("\n\n").find(|s| s.starts_with("Textos sueltos:")).unwrap();
        assert_eq!(sec, "Textos sueltos:\n- «texto libre segunda línea»");
    }

    #[test]
    fn imagen_una() {
        let out = describir(&json!({"elements": [{"id": "i", "type": "image"}]}));
        assert!(out.contains("Imágenes:\n- 1 imagen: si es un diagrama Mermaid que no se pudo pasar a formas, su contenido está en el código fuente."), "…{out}");
    }

    #[test]
    fn forma_sin_etiqueta() {
        let out = describir(&json!({"elements": [{"id": "a", "type": "rectangle"}]}));
        assert!(out.contains("Formas:\n- caja [caja]"), "…{out}");
    }
}
