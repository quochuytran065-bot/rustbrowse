//! Native Win32 Tab Strip with GDI rendering and hit-testing.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use common::types::{Rect, TabId, TabLifecycleState, TabMetadata};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetStockObject, SelectObject, SetBkMode,
    SetTextColor, DEFAULT_GUI_FONT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    HDC, TRANSPARENT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStripHit {
    None,
    NewTab,
    SelectTab(TabId),
    CloseTab(TabId),
}

#[derive(Debug, Clone)]
pub struct TabVisualItem {
    pub id: TabId,
    pub title: String,
    pub is_active: bool,
    pub state: TabLifecycleState,
    pub is_pinned: bool,
    pub rect: Rect,
    pub close_rect: Rect,
}

pub struct TabStrip {
    items: Vec<TabVisualItem>,
    new_tab_rect: Rect,
    height: i32,
}

impl TabStrip {
    pub const DEFAULT_HEIGHT: i32 = 36;

    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            new_tab_rect: Rect::new(0, 0, 0, 0),
            height: Self::DEFAULT_HEIGHT,
        }
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// Recomputes the geometric layout for tabs and the '+' button.
    pub fn update_layout(
        &mut self,
        tabs: &[&TabMetadata],
        active_id: Option<TabId>,
        client_width: i32,
    ) {
        self.items.clear();
        if tabs.is_empty() {
            self.new_tab_rect = Rect::new(8, 4, 34, 30);
            return;
        }

        let tab_count = tabs.len() as i32;
        let available_w = (client_width - 60).max(100);
        let max_tab_w = 200;
        let min_tab_w = 80;

        let computed_w = (available_w / tab_count).clamp(min_tab_w, max_tab_w);

        let mut current_x = 6;
        let tab_top = 4;
        let tab_bottom = self.height;

        for tab in tabs {
            let is_active = Some(tab.id) == active_id;
            let tab_w = if tab.is_pinned { 40 } else { computed_w };

            let tab_rect = Rect::new(current_x, tab_top, current_x + tab_w, tab_bottom);
            let close_rect = if tab.is_pinned {
                Rect::new(0, 0, 0, 0)
            } else {
                Rect::new(
                    current_x + tab_w - 24,
                    tab_top + 4,
                    current_x + tab_w - 2,
                    tab_bottom - 4,
                )
            };

            self.items.push(TabVisualItem {
                id: tab.id,
                title: if tab.title.is_empty() {
                    "Tab moi".to_string()
                } else {
                    tab.title.clone()
                },
                is_active,
                state: tab.state,
                is_pinned: tab.is_pinned,
                rect: tab_rect,
                close_rect,
            });

            current_x += tab_w + 3;
        }

        let max_x = (client_width - 36).max(10);
        let plus_x = current_x.min(max_x);
        self.new_tab_rect = Rect::new(plus_x + 4, tab_top + 2, plus_x + 34, tab_bottom - 2);
    }

    /// Hit tests a point (x, y) relative to the window client area.
    pub fn hit_test(&self, x: i32, y: i32) -> TabStripHit {
        if x == -1 && y == -1 {
            return TabStripHit::NewTab;
        }

        if y < 0 || y > self.height {
            return TabStripHit::None;
        }

        // Check '+' button
        if self.new_tab_rect.contains_point(x, y) {
            return TabStripHit::NewTab;
        }

        // Check tabs in reverse order (active on top)
        for item in self.items.iter().rev() {
            if !item.is_pinned && item.close_rect.contains_point(x, y) {
                return TabStripHit::CloseTab(item.id);
            }
            if item.rect.contains_point(x, y) {
                return TabStripHit::SelectTab(item.id);
            }
        }

        TabStripHit::None
    }

    /// Paints the tab strip using GDI in Fluent 2 / Windows 11 Light theme.
    pub fn paint(&self, hdc: HDC, client_width: i32) {
        unsafe {
            // Background bar (Fluent Light: #F8F9FA -> 0x00FAF9F8)
            let bg_rect = RECT {
                left: 0,
                top: 0,
                right: client_width,
                bottom: self.height,
            };
            // Dark theme Tab Strip background (#0B0D14)
            let bg_brush = CreateSolidBrush(COLORREF(0x00140D0B));
            FillRect(hdc, &bg_rect, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            let font = GetStockObject(DEFAULT_GUI_FONT);
            let old_font = SelectObject(hdc, font);
            SetBkMode(hdc, TRANSPARENT);

            for item in &self.items {
                // Tab pill background
                let tab_rc = RECT {
                    left: item.rect.left,
                    top: item.rect.top,
                    right: item.rect.right,
                    bottom: item.rect.bottom,
                };

                // Active: Sleek dark slate #1C2234, Inactive: Deeper obsidian #121622
                let tab_brush = if item.is_active {
                    CreateSolidBrush(COLORREF(0x0034221C))
                } else {
                    CreateSolidBrush(COLORREF(0x00221612))
                };
                FillRect(hdc, &tab_rc, tab_brush);
                let _ = DeleteObject(tab_brush.into());

                // Tab text color: Active is Neon Cyan #00F0FF, Inactive is Muted Slate #A0AEC0
                if item.is_active {
                    SetTextColor(hdc, COLORREF(0x00FFF000));
                } else {
                    SetTextColor(hdc, COLORREF(0x00C0AEA0));
                }

                let display_title = match item.state {
                    TabLifecycleState::Sleeping => format!("[💤 Sleep] {}", item.title),
                    TabLifecycleState::Discarded => format!("[Off] {}", item.title),
                    _ => item.title.clone(),
                };

                let mut text_rc = RECT {
                    left: item.rect.left + 8,
                    top: item.rect.top,
                    right: if item.is_pinned {
                        item.rect.right - 4
                    } else {
                        item.close_rect.left - 4
                    },
                    bottom: item.rect.bottom,
                };

                let mut wide_title: Vec<u16> = OsStr::new(&display_title)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                let _ = DrawTextW(
                    hdc,
                    &mut wide_title,
                    &mut text_rc,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );

                // Draw close 'x' button
                if !item.is_pinned {
                    SetTextColor(hdc, COLORREF(0x008A96A8)); // Close button gray
                    let mut close_rc = RECT {
                        left: item.close_rect.left,
                        top: item.close_rect.top,
                        right: item.close_rect.right,
                        bottom: item.close_rect.bottom,
                    };
                    let mut x_str: Vec<u16> = OsStr::new("x")
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    let _ = DrawTextW(
                        hdc,
                        &mut x_str,
                        &mut close_rc,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
            }

            // Draw '+' button
            let plus_rc = RECT {
                left: self.new_tab_rect.left,
                top: self.new_tab_rect.top,
                right: self.new_tab_rect.right,
                bottom: self.new_tab_rect.bottom,
            };
            let plus_brush = CreateSolidBrush(COLORREF(0x00221612));
            FillRect(hdc, &plus_rc, plus_brush);
            let _ = DeleteObject(plus_brush.into());

            SetTextColor(hdc, COLORREF(0x00FFF000));
            let mut plus_str: Vec<u16> = OsStr::new("+")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut plus_text_rc = plus_rc;
            let _ = DrawTextW(
                hdc,
                &mut plus_str,
                &mut plus_text_rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            SelectObject(hdc, old_font);
        }
    }
}

impl Default for TabStrip {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_strip_hit_testing() {
        let mut strip = TabStrip::new();
        let t1 = TabMetadata::new(TabId::new(1), "https://site1.com".to_string());
        let t2 = TabMetadata::new(TabId::new(2), "https://site2.com".to_string());

        let tabs = vec![&t1, &t2];
        strip.update_layout(&tabs, Some(TabId::new(1)), 1000);

        // Click outside tab bar
        assert_eq!(strip.hit_test(50, 100), TabStripHit::None);

        // Click on Tab 1 body
        let hit1 = strip.hit_test(30, 15);
        assert_eq!(hit1, TabStripHit::SelectTab(TabId::new(1)));

        // Click on '+' button
        let plus_hit = strip.hit_test(strip.new_tab_rect.left + 2, strip.new_tab_rect.top + 2);
        assert_eq!(plus_hit, TabStripHit::NewTab);
    }
}
