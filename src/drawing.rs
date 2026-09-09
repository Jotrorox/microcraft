use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

#[derive(PartialEq, Eq)]
pub struct ResourceUsage {
    pub heap_used: usize,
    pub heap_free: usize,
}

impl ResourceUsage {
    pub fn current() -> Self {
        let stats = esp_alloc::HEAP.stats();
        let mut heap_used = 0;
        let mut heap_free = 0;

        for region in stats.region_stats.iter().flatten() {
            heap_used += region.used;
            heap_free += region.free;
        }

        Self {
            heap_used,
            heap_free,
        }
    }

    pub fn heap_percent(&self) -> usize {
        let total = self.heap_used + self.heap_free;
        (self.heap_used * 100).checked_div(total).unwrap_or(0)
    }
}

pub fn draw_status<D>(display: &mut D, ip: &str, usage: &ResourceUsage) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    display.clear(BinaryColor::Off)?;

    draw_line(display, 10, format_args!("Microcraft"))?;
    draw_line(display, 24, format_args!("IP: {ip}"))?;
    let heap_percent = usage.heap_percent().min(100);
    draw_line(display, 36, format_args!("Heap: {heap_percent}%"))?;
    draw_line(display, 48, format_args!("Free: {} B", usage.heap_free))?;
    draw_bar(display, heap_percent)
}

fn draw_line<D: DrawTarget<Color = BinaryColor>>(
    display: &mut D,
    y: i32,
    args: core::fmt::Arguments<'_>,
) -> Result<(), D::Error> {
    let mut line = String::<32>::new();
    core::fmt::write(&mut line, args).expect("status line fits 32 bytes");
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new(&line, Point::new(0, y), style).draw(display)?;
    Ok(())
}

fn draw_bar<D: DrawTarget<Color = BinaryColor>>(
    display: &mut D,
    heap_percent: usize,
) -> Result<(), D::Error> {
    let bar_width = 120u32;
    let inner_width = bar_width - 2;
    let fill_width = inner_width * heap_percent as u32 / 100;

    Rectangle::new(Point::new(0, 55), Size::new(bar_width, 8))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display)?;

    if fill_width > 0 {
        Rectangle::new(Point::new(1, 56), Size::new(fill_width, 6))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display)?;
    }

    Ok(())
}
