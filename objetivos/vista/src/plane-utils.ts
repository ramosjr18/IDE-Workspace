// Lo único de @plane/utils que usan las piezas de Plane que reutilizamos: `cn`, copiado de
// objetivos/plane/packages/utils/src/common.ts (AGPL-3.0, Plane Software, Inc. and contributors).
// Su paquete entero arrastra muchas dependencias que la vista no necesita.
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export const cn = (...inputs: ClassValue[]) => twMerge(clsx(inputs));
