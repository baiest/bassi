# Voz de Nala

`reference.wav` es el audio de referencia que Chatterbox usa para clonar la
voz de Nala. No se incluye en el repo por defecto — si falta, Nala hace
fallback automático a Windows SAPI (ver `NALA_TTS` en el README principal).

## Cómo grabar una referencia

- 10-20 segundos de una sola voz, sin música ni ruido de fondo.
- Mono, 24 kHz o más.
- Habla natural y continua (evita silencios largos o muletillas).
- Guarda el archivo como `data/voices/nala/reference.wav`.

## Cómo usar otra referencia

Sin mover archivos, apunta `NALA_CHATTERBOX_REFERENCE` a la ruta que
quieras:

```powershell
$env:NALA_CHATTERBOX_REFERENCE = "C:\ruta\a\otra_voz.wav"
```
