;; Mixed outline for URDF that preserves hierarchy:
;; - robot / link / joint show their tag name plus the `name` attribute value
;; - other XML/URDF elements still appear in the outline
;; - hierarchy is preserved because @item is attached to the whole element node

;; --- Special cases: robot / link / joint with name attribute shown ---

(element
  (STag
    (Name) @name
    (Attribute
      (Name) @_attr_name
      (AttValue) @context.extra))
  (#match? @name "^(robot|link|joint)$")
  (#eq? @_attr_name "name")) @item

(EmptyElemTag
  (Name) @name
  (Attribute
    (Name) @_attr_name
    (AttValue) @context.extra)
  (#match? @name "^(robot|link|joint)$")
  (#eq? @_attr_name "name")) @item

;; --- Fallback: keep other normal elements in the outline ---

((element
  (STag
    (Name) @name)) @item
  (#not-match? @name "^(robot|link|joint)$"))

((EmptyElemTag
  (Name) @name) @item
  (#not-match? @name "^(robot|link|joint)$"))

;; --- Keep doctype if present ---

(doctypedecl
  (Name) @name) @item
