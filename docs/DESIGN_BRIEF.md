# Design Brief — tnls (SSH Tunnel Manager)

**Dự án:** Desktop app quản lý SSH tunnels  
**Tech stack:** Tauri 2 + React + TypeScript + Rust  
**Target users:** Developers, DevOps engineers  
**Date:** November 9, 2026

---

## 1. Tổng quan sản phẩm

**tnls** là ứng dụng desktop nhẹ giúp quản lý SSH tunnels: lưu tunnel profiles, bật/tắt bằng UI, chạy nền qua system tray. Mục tiêu là thay thế việc mở nhiều terminal sessions để chạy lệnh SSH thủ công.

### Core Features (MVP)
- CRUD tunnel profiles (local/remote/dynamic forwarding)
- Start/stop/restart tunnels với status tracking
- Runtime status: Stopped/Starting/Running/Stopping/Failed/Degraded
- SQLite persistence
- System tray integration (đóng window ≠ thoát app)
- Log viewer cho từng tunnel
- Auto-start tunnels khi launch app
- Import/export JSON, import from `~/.ssh/config`
- Bulk actions (start/stop all visible)
- Keyboard shortcuts: `/` search, `Ctrl+N` new tunnel, `Esc` close

---

## 2. Vấn đề UX/UI hiện tại

### 2.1 Topbar
- **Layout:** Header bên trái (title + stats), search + actions bên phải
- **Issues có thể cải thiện:**
  - Spacing giữa các elements chưa balance
  - Stats text (`5 tunnels · 2 running`) có thể highlight hơn (ví dụ: status badge cho running count)
  - Search input có thể có visual indicator khi active (border color, icon)
  - Nút "Refresh" và "New tunnel" không có visual hierarchy rõ ràng

### 2.2 Tunnel List Table
- **Layout:** Table với 7 columns (Name, Mode, Local, Remote, Status, Detail, Actions)
- **Issues có thể cải thiện:**
  - **Row actions quá nhiều nút:** Start/Stop/Restart/Edit/Duplicate/Logs/Delete → 6-7 buttons trong 1 cell
  - Khi confirm delete, 2 nút thêm (Confirm/Cancel) làm layout break
  - Status badge chỉ có màu + text, có thể thêm icon để visual rõ hơn
  - Detail column có thể truncate quá mức, user phải hover để xem full text
  - Copyable local/remote addresses có thể có icon copy thay vì chỉ click-to-copy
  - Row hover/focus state chưa rõ ràng
  - Dark mode support có nhưng chưa refined

### 2.3 Status Badge
- **Current:** Text badge với dot indicator + color coding
- **Issues có thể cải thiện:**
  - Animation cho state transitions (starting → running, running → failed)
  - "Starting" và "Stopping" có thể có spinner thay vì chỉ màu vàng
  - "Degraded" và "Unknown" cùng màu cam, có thể differentiate

### 2.4 Log Drawer
- **Layout:** Slide-in drawer từ phải, dark background, preformatted text
- **Issues có thể cải thiện:**
  - Live toggle + Refresh button có thể gọn hơn (icon buttons)
  - No search/filter trong logs
  - No timestamp formatting
  - No color coding cho log levels (ERROR, WARN, INFO)
  - Scrolling experience khi logs dài

### 2.5 Tunnel Form (Edit/Create)
- **Layout:** Form drawer với nhiều fields, grid layout 2 columns cho một số rows
- **Issues có thể cải thiện:**
  - **Quá nhiều fields trong 1 form dài:** 13+ input fields, cần scroll trên màn nhỏ
  - Passphrase field logic phức tạp (stored state, clear button, message)
  - No field grouping/sections (Basic, SSH Config, Advanced)
  - Error messages xuất hiện dưới từng field nhưng không scroll to error
  - No preview của SSH command sẽ được generate
  - Identity file, ProxyJump có thể có file picker/SSH config selector

### 2.6 Empty State
- **Current:** Simple card với text + code example
- **Issues có thể cải thiện:**
  - Có thể có illustration hoặc icon
  - Quick actions (Import from ~/.ssh/config, Create first tunnel)

### 2.7 Other UX Issues
- **Bulk actions** placement có thể tốt hơn (sticky bar khi scroll?)
- **Import/Export buttons** nằm rải rác (topbar vs separate row)
- **Keyboard shortcuts hints** có thể visible hơn
- **Confirmation dialogs** hiện tại inline (delete), có thể dùng modal cho consistency
- **Autostart checkbox** có thể move vào Settings/Preferences thay vì prominent ở main view

---

## 3. User Journey chính

### 3.1 Tạo tunnel mới
1. User click "New tunnel" hoặc `Ctrl+N`
2. Form drawer slide in từ phải
3. User điền name, SSH target, ports
4. (Optional) Config advanced: identity file, ProxyJump, keep-alive, retry
5. Click "Create tunnel" → save to SQLite → close drawer → refresh list

### 3.2 Start tunnel
1. Tunnel ở state "Stopped" hoặc "Failed"
2. User click "Start"
3. Badge → "Starting" (yellow) → "Running" (green)
4. PID + uptime xuất hiện ở Detail column

### 3.3 Monitor & Debug
1. User click "Logs" button
2. Log drawer slide in
3. Real-time logs update (Live mode)
4. User có thể toggle live off, manual refresh

### 3.4 Edit/Delete
1. User click "Edit" → form drawer với prefilled data
2. Hoặc "Delete" → inline confirm → remove from list

---

## 4. Target visual style

### 4.1 Current Design Language
- **Color palette:** Neutral grays, minimal color (blue accent for hover, red for danger)
- **Typography:** Inter font family, 15px base, monospace for code/ports
- **Border radius:** 8px (buttons), 12px (cards/table), 999px (badges)
- **Shadows:** Minimal, chỉ drawer có shadow
- **Spacing:** 8-16px gaps, 20-24px padding

### 4.2 Aspirations
- **Modern, clean, functional** — developer tools aesthetic
- **Dark mode first-class citizen** — hiện đã có nhưng cần refine
- **Visual hierarchy rõ ràng** — primary actions vs secondary
- **Micro-interactions** — transitions, hover states, loading states
- **Accessible** — keyboard navigation, screen reader friendly

---

## 5. Screens cần design/redesign

### 5.1 Main Window
- Topbar với stats, search, actions
- Tunnel list table
- Empty state
- Bulk actions bar
- Import/export row

### 5.2 Tunnel Form Drawer
- Create new tunnel
- Edit existing tunnel
- Field grouping (Basic, SSH, Advanced)
- Error states
- Success states

### 5.3 Log Drawer
- Log viewer với timestamps
- Search/filter controls
- Live mode toggle
- Color-coded log levels

### 5.4 Status Components
- Status badge với icons
- Status transitions (animations)
- Row status indicators (left border color?)

### 5.5 Confirmations & Modals
- Delete confirmation modal
- Import success/error toast
- Action feedback

### 5.6 System Tray Menu (Tauri)
- Quick actions (start all, stop all, open window)
- Running tunnels list

---

## 6. Specific pain points cần address

### 6.1 Actions column quá chật
**Problem:** 6-7 buttons trong 1 table cell, layout break khi confirm delete

**Possible solutions:**
- Group actions vào overflow menu (⋯)
- Move Edit/Delete/Duplicate vào context menu (right-click)
- Use icon buttons thay vì text buttons
- Quick actions column + expandable detail row

### 6.2 Form quá dài, không có structure
**Problem:** 13+ fields trong vertical layout, scroll fatigue

**Possible solutions:**
- Split thành tabs/sections (Basic, SSH Config, Advanced)
- Collapsible advanced fields
- Sidebar form layout (labels left, inputs right)
- Wizard pattern cho new users

### 6.3 Import/Export buttons placement
**Problem:** Separate row dưới topbar, dễ miss

**Possible solutions:**
- Move vào overflow menu (⋯) bên cạnh "New tunnel"
- Dedicated settings/preferences view
- Keyboard shortcut hints

### 6.4 Visual hierarchy
**Problem:** All buttons look similar, không có clear primary action

**Possible solutions:**
- Primary button style cho "New tunnel", "Save"
- Ghost/outline cho secondary actions
- Icon + text cho important actions
- Color coding cho dangerous actions (Delete)

---

## 7. Technical constraints

- **Frontend framework:** React 19 + TypeScript
- **Styling:** Plain CSS (hiện tại), có thể migrate sang:
  - Tailwind CSS (utility-first)
  - CSS Modules (scoped)
  - Styled-components (CSS-in-JS)
- **UI Components:** Hiện tại self-built, có thể dùng:
  - Radix UI (headless, accessible)
  - shadcn/ui (Radix + Tailwind)
  - Headless UI
- **Icons:** Hiện tại không có icon library, có thể dùng:
  - Lucide React
  - Heroicons
  - Phosphor Icons
- **Dark mode:** CSS custom properties với `prefers-color-scheme`
- **Accessibility:** ARIA labels đã có, cần audit

---

## 8. Deliverables mong đợi từ design agent

1. **High-fidelity mockups** cho tất cả screens (desktop, dark mode)
2. **Component redesign:**
   - Tunnel table với better actions layout
   - Status badges với icons + animations
   - Form drawer với sections/grouping
   - Log drawer với search + color coding
3. **Design system specs:**
   - Color palette (light + dark)
   - Typography scale
   - Spacing scale
   - Component specs (buttons, inputs, badges, etc.)
4. **Interaction design:**
   - Hover states
   - Loading states
   - Error states
   - Transitions
5. **Accessibility audit** recommendations

---

## 9. Resources

- **Repo:** `c:\Users\khanhnv\Projects\tnls`
- **Design doc:** `docs/DESIGN.md`
- **Current UI:** `src/App.tsx`, `src/components/`
- **Styles:** `src/App.css`
- **Types:** `src/lib/types.ts`
- **API layer:** `src/lib/api.ts`

---

## 10. Timeline & Priority

**High priority:**
- Fix actions column layout
- Improve form structure
- Better visual hierarchy

**Medium priority:**
- Log drawer enhancements
- Status badge animations
- Empty state illustration

**Low priority (nice to have):**
- Keyboard shortcuts panel
- Settings/preferences view
- System tray menu redesign

---

## 11. Contact & Collaboration

**Developer:** User sẽ implement design changes  
**Design iteration:** Review mockups → feedback → refine → implement  
**Communication:** Share Figma/Sketch files, design specs document

---

_Đây là baseline brief. Design agent có thể propose alternative solutions hoặc ask clarifying questions._
