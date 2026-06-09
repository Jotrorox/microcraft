use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

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
        if total == 0 {
            0
        } else {
            self.heap_used * 100 / total
        }
    }
}

pub fn draw_status<D>(display: &mut D, ip: &str, usage: &ResourceUsage) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    display.clear(BinaryColor::Off)?;

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("Microcraft", Point::new(0, 10), style).draw(display)?;

    let mut ip_line: String<48> = String::new();
    let _ = core::fmt::write(&mut ip_line, format_args!("IP: {ip}"));
    Text::new(&ip_line, Point::new(0, 24), style).draw(display)?;

    let heap_percent = usage.heap_percent().min(100);

    let mut heap_line: String<48> = String::new();
    let _ = core::fmt::write(&mut heap_line, format_args!("Heap: {heap_percent}%"));
    Text::new(&heap_line, Point::new(0, 36), style).draw(display)?;

    let mut free_line: String<48> = String::new();
    let _ = core::fmt::write(&mut free_line, format_args!("Free: {} B", usage.heap_free));
    Text::new(&free_line, Point::new(0, 48), style).draw(display)?;

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
