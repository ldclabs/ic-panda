# Política de privacidad de dMsg

Última actualización: 2026-10-11.

dMsg está diseñado para que tu contenido privado se cifre en tus propios dispositivos. Esta política explica qué información manejan el sitio web de dMsg, la aplicación web antigua, la extensión de navegador, el servicio en la nube y los canisters en Internet Computer (ICP), quién puede verla y qué opciones tienes.

No vendemos datos personales, no usamos tu contenido para publicidad y el sitio web no contiene rastreadores de analítica ni de publicidad.

## 1. Alcance

Esta política cubre dmsg.net, la aplicación web antigua de solo lectura, la extensión de navegador de dMsg, el servicio en la nube de dMsg y los canisters de dMsg. «Nosotros» se refiere al equipo que opera dMsg bajo la gobernanza de ICPanda DAO. Las billeteras, Internet Identity, los libros contables de tokens y las aplicaciones que conectas tienen sus propias prácticas de privacidad.

## 2. Contenido privado

- Las notas, mensajes, archivos y elementos de la bóveda se cifran en tus dispositivos aprobados y desbloqueados antes de subirse. La nube solo almacena y entrega texto cifrado.
- Las claves de contenido permanecen en tus dispositivos, envueltas para cada dispositivo que apruebas. Para la recuperación, la cuenta también guarda un sobre que solo puede abrirse mediante el servicio vetKD de ICP según las reglas de recuperación de la cuenta.
- No recibimos tu texto sin cifrar ni tus claves de contenido.
- Las personas con quienes compartes contenido pueden leerlo y conservar copias.

## 3. Información no cifrada

El cifrado protege el contenido, no todas las relaciones ni acciones. Para operar el servicio, el servicio en la nube y los canisters procesan:

- ID de cuenta, principals de inicio de sesión, claves públicas e identificadores de dispositivos;
- nombres, perfiles públicos y las reglas de contacto que configuras;
- la pertenencia a canales y las relaciones de contacto necesarias para aplicar tus reglas;
- el tamaño y el momento de objetos, mensajes y solicitudes;
- las direcciones IP y los datos de las solicitudes que llegan al servicio en la nube;
- registros de pedidos, pagos, reembolsos y derechos.

## 4. Información pública y en cadena

Parte de la información es pública por diseño:

- los nombres y las cuentas que los poseen;
- los documentos del directorio servidos en id.dmsg.net, incluidos sus controladores, su servicio de delegación y el enlace al perfil;
- los datos de perfil que decides publicar;
- las transacciones en los libros contables de tokens de ICP, incluidos los pagos a dMsg y los reembolsos de dMsg.

El estado de la cuenta que otros servicios deben verificar, como los derechos del plan y los recibos de firmas formales, se publica como datos certificados. Los registros escritos en ICP generalmente no pueden borrarse.

## 5. Pagos y membresía PANDA

Pagas desde tu propia billetera en libros contables de ICP como ckUSDC y PANDA. Los canisters commerce, payment y handle de dMsg guardan registros de pedidos, depósitos en garantía, ingresos, reembolsos y recibos, incluidos la cuenta que paga y los importes. Si solicitas una exención con PANDA, el canister membership lee de la SNS de PANDA la participación, el estado de disolución y los principals de tu neurona.

## 6. Datos en tu dispositivo

- La extensión guarda en el navegador tus claves, una base de datos local cifrada, los cambios pendientes de enviar, el estado de sincronización y la configuración.
- El sitio web guarda tu idioma elegido en el almacenamiento local.
- La aplicación antigua guarda tu sesión y datos en caché en IndexedDB del navegador, y su service worker almacena en caché los archivos del sitio. Su vista de billetera solicita precios de tokens a la API de ICPSwap.

Desinstalar la extensión o borrar los datos del sitio elimina estas copias locales. El contenido que no se sincronizó con la nube se pierde con ellas.

## 7. Proveedores de servicios

Usamos los siguientes proveedores solo en la medida necesaria para operar dMsg:

- Cloudflare ejecuta el servicio en la nube y procesa el texto cifrado y los datos de las solicitudes que recibe.
- Los proveedores de nodos de ICP ejecutan los canisters, y las pasarelas HTTP sirven dmsg.net e id.dmsg.net. Pueden conservar registros de solicitudes estándar.
- Internet Identity y las billeteras que elijas gestionan tu inicio de sesión y tus pagos.
- Chrome Web Store distribuye la extensión.

Podemos divulgar información cuando la ley lo exija. Como el contenido está cifrado, solo podemos divulgar el texto cifrado y los metadatos que tenemos.

## 8. Conservación

- El texto cifrado permanece en la nube mientras tu cuenta lo usa y según lo permita tu plan. Cuando eliminas un elemento, su texto cifrado se retira según las reglas que muestra la extensión.
- Cuando termina un plan de pago, los Términos del servicio describen qué ocurre con el contenido que supera los límites gratuitos.
- Los registros operativos se conservan solo el tiempo necesario para la seguridad, la prevención de abusos y la resolución de problemas.
- Los nombres, las transacciones de los libros contables, los recibos de firma y otros registros escritos en ICP los conservan los canisters y los libros contables, y generalmente no pueden borrarse.

## 9. Tus opciones y derechos

En la extensión puedes exportar tus datos, eliminar elementos, revocar dispositivos y cambiar o cerrar tus reglas de contacto. Según dónde vivas, puedes tener derecho a acceder a tu información personal, corregirla o eliminarla, o a oponerte a su uso. Contáctanos a través de la página de soporte. Como no podemos descifrar tu contenido ni borrar registros en cadena, algunas solicitudes solo pueden atenderse en parte.

## 10. Seguridad y sus límites

dMsg usa cifrado en el dispositivo, aprobaciones separadas para el inicio de sesión, los dispositivos y las firmas, y datos certificados que los clientes pueden verificar. Ningún sistema es perfectamente seguro. Los canisters de dMsg están controlados por la gobernanza de ICPanda DAO, que puede actualizarlos y cambiar sus reglas, y las actualizaciones de la extensión son otro punto de confianza. Informa de vulnerabilidades en privado a través de la página de soporte.

## 11. Menores

dMsg no está destinado a menores de 13 años ni a quienes no tengan la edad mínima exigida en su país para usar servicios en línea sin consentimiento parental.

## 12. Cambios y contacto

Cuando esta política cambie, actualizaremos la fecha indicada arriba y anunciaremos los cambios importantes en el sitio web o en la extensión. Para preguntas o solicitudes, consulta [Soporte](/_/support).
