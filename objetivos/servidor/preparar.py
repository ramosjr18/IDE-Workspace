# Prepara Plane para la app: usuario local, espacio de trabajo «objetivos» y una clave de API.
# Se ejecuta dentro del contenedor de la API (manage.py shell). Se puede repetir: no duplica nada.
# Imprime la clave de API en la última línea.
import os
from plane.db.models import User, Workspace, WorkspaceMember, APIToken
from plane.license.models import Instance

EMAIL = os.environ.get("OBJETIVOS_EMAIL", "local@objetivos.local")
SLUG = "objetivos"

# Instancia: configurada y sin telemetría.
inst = Instance.objects.first()
if inst:
    inst.is_telemetry_enabled = False
    inst.is_setup_done = True
    inst.save()

user, created = User.objects.get_or_create(email=EMAIL, defaults={"username": "local", "first_name": "Local", "display_name": "local", "is_active": True})
if created:
    user.set_unusable_password()
    user.save()

ws, _ = Workspace.objects.get_or_create(slug=SLUG, defaults={"name": "Objetivos", "owner": user})
WorkspaceMember.objects.get_or_create(workspace=ws, member=user, defaults={"role": 20})

token = APIToken.objects.filter(user=user, workspace=ws, label="chat-de-agentes", is_active=True).first()
if not token:
    token = APIToken.objects.create(user=user, workspace=ws, label="chat-de-agentes", description="Pestaña Objetivo de la app")
print(token.token)
