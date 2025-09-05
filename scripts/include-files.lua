-- include-files.lua
-- A lightweight implementation compatible with pandoc/lua-filters include-files.
-- It expands lines of the form `!include path/to/file.md` that appear as
-- standalone paragraphs by replacing them with the parsed blocks of the target file.
-- Paths are resolved relative to the including document's directory.

local stringify = pandoc.utils.stringify

-- Return the directory part of a path (with trailing slash if non-empty)
local function dirname(path)
  if not path then return "" end
  -- normalize separators to '/'
  path = path:gsub('\\', '/')
  local dir = path:match('^(.*)/') or ''
  if dir ~= '' and dir:sub(-1) ~= '/' then
    dir = dir .. '/'
  end
  return dir
end

-- Resolve a path relative to the main input file's directory when not absolute
local function resolve_path(path)
  if not path or path == '' then return path end
  -- absolute (POSIX)
  if path:sub(1,1) == '/' then return path end
  -- absolute (Windows drive letter)
  if path:match('^%a:[/\\]') then return path end
  local input = PANDOC_STATE and PANDOC_STATE.input_files and PANDOC_STATE.input_files[1]
  local base = dirname(input)
  return base .. path
end

local function try_read(path)
  local fh, err = io.open(path, 'r')
  if not fh then
    io.stderr:write('[include-files] cannot open ' .. tostring(path) .. ': ' .. tostring(err) .. '\n')
    return nil
  end
  local content = fh:read('*a')
  fh:close()
  return content
end

function Para(el)
  local text = stringify(el)
  local inc = text:match('^!include%s+(.+)$')
  if not inc then return nil end
  inc = inc:gsub('^%s+', ''):gsub('%s+$', '')
  local path = resolve_path(inc)
  local content = try_read(path)
  if not content then
    -- leave the original paragraph untouched if include fails
    return nil
  end
  local doc = pandoc.read(content, 'markdown')
  return doc.blocks
end
