use axum::response::Html;

const BASE_HTML: &str = r#"<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f7f1e8;
      --paper: #fffaf3;
      --ink: #2a2118;
      --muted: #6b5c4d;
      --accent: #c95b2b;
      --line: #e7d8c5;
    }}
    * {{
      box-sizing: border-box;
    }}
    body {{
      margin: 0;
      font-family: Georgia, "Times New Roman", serif;
      background:
        radial-gradient(circle at top, rgba(201, 91, 43, 0.10), transparent 38%),
        linear-gradient(180deg, #fbf5eb 0%, var(--bg) 100%);
      color: var(--ink);
      line-height: 1.65;
    }}
    main {{
      max-width: 860px;
      margin: 0 auto;
      padding: 48px 20px 72px;
    }}
    .card {{
      background: var(--paper);
      border: 1px solid var(--line);
      border-radius: 24px;
      padding: 32px;
      box-shadow: 0 20px 60px rgba(42, 33, 24, 0.08);
    }}
    .eyebrow {{
      margin: 0 0 8px;
      color: var(--accent);
      font-size: 0.82rem;
      font-weight: 700;
      letter-spacing: 0.12em;
      text-transform: uppercase;
    }}
    h1, h2 {{
      line-height: 1.2;
    }}
    h1 {{
      margin: 0 0 12px;
      font-size: clamp(2rem, 4vw, 3.3rem);
    }}
    h2 {{
      margin-top: 32px;
      font-size: 1.3rem;
    }}
    p, li {{
      font-size: 1.02rem;
    }}
    .intro {{
      color: var(--muted);
      font-size: 1.08rem;
      margin-bottom: 28px;
    }}
    ul {{
      padding-left: 20px;
    }}
    a {{
      color: var(--accent);
    }}
    footer {{
      margin-top: 28px;
      color: var(--muted);
      font-size: 0.95rem;
    }}
    @media (max-width: 640px) {{
      main {{
        padding: 24px 14px 40px;
      }}
      .card {{
        padding: 22px;
        border-radius: 18px;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <p class="eyebrow">Trabix Granizados</p>
      {content}
      <footer>
        Contacto: WhatsApp <a href="https://wa.me/573043535455">+57 304 353 5455</a><br>
        Ultima actualizacion: 12 de marzo de 2026
      </footer>
    </section>
  </main>
</body>
</html>
"#;

pub async fn privacy_policy() -> Html<String> {
    Html(render_page(
        "Politica de Privacidad | Trabix Granizados",
        r#"
<h1>Politica de Privacidad</h1>
<p class="intro">
  Esta politica explica como Trabix Granizados recopila y usa la informacion que compartes
  al hacer pedidos por WhatsApp mediante nuestro bot de atencion.
</p>

<h2>1. Informacion que recopilamos</h2>
<ul>
  <li>Nombre y numero de telefono.</li>
  <li>Direccion de entrega y datos del pedido.</li>
  <li>Horario solicitado para entrega inmediata o programada.</li>
  <li>Metodo de pago seleccionado.</li>
  <li>Imagen del comprobante de pago, si eliges pagar por transferencia.</li>
  <li>Mensajes e interacciones necesarias para atender tu pedido o conectarte con un asesor.</li>
</ul>

<h2>2. Para que usamos la informacion</h2>
<ul>
  <li>Registrar, confirmar y entregar pedidos.</li>
  <li>Calcular valores estimados y coordinar el domicilio.</li>
  <li>Dar soporte por WhatsApp y derivar el caso a un asesor cuando sea necesario.</li>
  <li>Resolver incidentes operativos, prevenir abuso y mejorar el servicio.</li>
</ul>

<h2>3. Como compartimos la informacion</h2>
<p>
  Solo compartimos la informacion necesaria para operar el servicio con plataformas y personas
  involucradas en la atencion del pedido, como Meta/WhatsApp, la infraestructura tecnica del bot,
  el asesor comercial y el personal encargado del domicilio o la preparacion del pedido.
  No vendemos tu informacion personal.
</p>

<h2>4. Conservacion de datos</h2>
<p>
  Conservamos la informacion mientras sea necesaria para operar pedidos, atender soporte,
  llevar control comercial basico y cumplir obligaciones legales o contables cuando apliquen.
</p>

<h2>5. Tus derechos</h2>
<p>
  Puedes solicitar correccion, actualizacion o eliminacion de tus datos escribiendo al mismo canal
  de WhatsApp del servicio. Revisaremos cada solicitud de forma razonable segun las obligaciones
  operativas y legales aplicables.
</p>

<h2>6. Menores de edad y bebidas con licor</h2>
<p>
  Algunos productos pueden incluir licor. Al solicitar productos con licor, declaras ser mayor de edad
  y autorizas el tratamiento de la informacion necesaria para validar y gestionar ese pedido.
</p>

<h2>7. Cambios a esta politica</h2>
<p>
  Podemos actualizar esta politica para reflejar cambios operativos, legales o tecnicos.
  La version vigente sera la publicada en esta pagina.
</p>
"#,
    ))
}

pub async fn terms_of_service() -> Html<String> {
    Html(render_page(
        "Terminos del Servicio | Trabix Granizados",
        r#"
<h1>Terminos del Servicio</h1>
<p class="intro">
  Estos terminos regulan el uso del canal de pedidos por WhatsApp de Trabix Granizados.
  Al usar el bot o continuar la conversacion con un asesor, aceptas estas condiciones.
</p>

<h2>1. Descripcion del servicio</h2>
<p>
  Trabix Granizados ofrece toma de pedidos por WhatsApp para granizados a domicilio.
  El bot puede mostrar menu, tomar datos, registrar pedidos, gestionar pagos y derivar la conversacion
  a un asesor cuando el caso lo requiera.
</p>

<h2>2. Cobertura y disponibilidad</h2>
<p>
  La disponibilidad del servicio, los horarios de atencion y la cobertura de domicilios pueden variar
  segun la zona, la demanda y la operacion del negocio. Un asesor puede confirmar costos, tiempos
  y disponibilidad final antes del cierre del pedido.
</p>

<h2>3. Precios y pagos</h2>
<ul>
  <li>Los precios mostrados por el bot pueden ser estimados y no siempre incluyen el domicilio.</li>
  <li>El valor final del domicilio puede ser confirmado por un asesor.</li>
  <li>Los pagos pueden realizarse contra entrega o por transferencia, segun las opciones habilitadas.</li>
  <li>Si el pedido requiere comprobante, el cliente debe enviar una imagen legible.</li>
</ul>

<h2>4. Obligaciones del cliente</h2>
<ul>
  <li>Entregar informacion real y suficiente para procesar el pedido.</li>
  <li>Revisar direccion, telefono y detalles del pedido antes de confirmarlo.</li>
  <li>No usar el servicio para fines fraudulentos, ofensivos o contrarios a la ley.</li>
</ul>

<h2>5. Pedidos con licor</h2>
<p>
  Los productos con licor solo pueden ser solicitados por personas mayores de edad.
  Al pedirlos, declaras cumplir ese requisito y aceptas cualquier validacion razonable
  que el negocio necesite para completar la venta.
</p>

<h2>6. Cancelaciones y rechazos</h2>
<p>
  Trabix Granizados puede rechazar o cancelar pedidos cuando haya datos incompletos, imposibilidad
  de entrega, falta de disponibilidad, sospecha de fraude, incumplimiento de estos terminos
  o ausencia de confirmacion de pago cuando sea requerida.
</p>

<h2>7. Limitacion del servicio</h2>
<p>
  Hacemos esfuerzos razonables para mantener el bot disponible, pero pueden ocurrir pausas,
  errores tecnicos, demoras en WhatsApp o intervenciones manuales del asesor. El negocio no garantiza
  disponibilidad continua e ininterrumpida del canal automatizado.
</p>

<h2>8. Contacto</h2>
<p>
  Si tienes dudas sobre un pedido, un pago o estos terminos, puedes escribir al canal oficial
  de WhatsApp de Trabix Granizados.
</p>
"#,
    ))
}

fn render_page(title: &str, content: &str) -> String {
    BASE_HTML
        .replace("{title}", title)
        .replace("{content}", content)
}
