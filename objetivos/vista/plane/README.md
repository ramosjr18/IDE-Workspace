# Piezas de Plane

Copia de `packages/blocks/src/layout` y `packages/blocks/src/icons` de **Plane** (v1.5.0-rc1), de
Plane Software, Inc. y colaboradores, bajo la licencia **AGPL-3.0-only** (`LICENSE.txt`,
`COPYRIGHT.txt`). Los archivos conservan sus cabeceras de copyright.

La pestaña Objetivo (`objetivos/vista`) las usa para tener la misma interfaz que Plane. Se copian aquí
para que la vista se compile en cualquier sitio (la estable y las releases de GitHub) sin la copia
completa de Plane de `objetivos/plane`, que no está en git. Ver `docs/plan-plane-objetivo.md`.

Para actualizarlas desde esa copia:

```bash
rm -rf objetivos/vista/plane/blocks/src/{layout,icons}
cp -r objetivos/plane/packages/blocks/src/{layout,icons} objetivos/vista/plane/blocks/src/
```
