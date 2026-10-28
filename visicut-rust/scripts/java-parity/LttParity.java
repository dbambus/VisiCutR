/*
 * Reference generator for the Java-parity tests of VisiCutR.
 *
 * Runs each case of visicut-rust/tests/java_parity through the original
 * Java pipeline (VisiCut SVG import, VectorProfile/RasterProfile and the
 * LibLaserCut LaserToolsTechnicsCutter of the FAU device) and writes the
 * resulting LTT bytes as expected.ltt next to the case. Started by
 * generate.sh; see the README in this folder.
 *
 * LGPL-3.0-or-later, like VisiCut and LibLaserCut.
 */

import de.thomas_oster.liblasercut.LaserCutter;
import de.thomas_oster.liblasercut.properties.LaserProperty;
import de.thomas_oster.liblasercut.dithering.DitheringAlgorithm;
import de.thomas_oster.visicut.VisicutModel;
import de.thomas_oster.visicut.managers.LaserDeviceManager;
import de.thomas_oster.visicut.model.LaserDevice;
import de.thomas_oster.visicut.model.LaserProfile;
import de.thomas_oster.visicut.model.PlfPart;
import de.thomas_oster.visicut.model.Raster3dProfile;
import de.thomas_oster.visicut.model.RasterProfile;
import de.thomas_oster.visicut.model.VectorProfile;
import de.thomas_oster.visicut.model.mapping.Mapping;
import de.thomas_oster.visicut.model.mapping.MappingSet;
import java.awt.geom.AffineTransform;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Properties;

public class LttParity
{
  public static void main(String[] args) throws Exception
  {
    if (args.length < 2)
    {
      System.err.println("usage: LttParity <device.xml> <case-dir>...");
      System.exit(2);
    }
    LaserDevice device = LaserDeviceManager.getInstance().loadFromFile(new File(args[0]));
    int failed = 0;
    for (int i = 1; i < args.length; i++)
    {
      File dir = new File(args[i]);
      Properties c = new Properties();
      try (InputStreamReader in = new InputStreamReader(new FileInputStream(new File(dir, "case.properties")), StandardCharsets.UTF_8))
      {
        c.load(in);
      }
      for (Map.Entry<String, Properties> variant : variants(c).entrySet())
      {
        String suffix = variant.getKey();
        try
        {
          generate(device, dir, variant.getValue(), suffix);
        }
        catch (Throwable t)
        {
          // A case Java cannot process: record the reason instead of stopping.
          failed++;
          new File(dir, "expected" + suffix + ".ltt").delete();
          StringBuilder reason = new StringBuilder();
          for (Throwable cause = t; cause != null; cause = cause.getCause())
          {
            if (reason.length() > 0)
            {
              reason.append(" <- ");
            }
            reason.append(cause.getClass().getSimpleName()).append(": ").append(cause.getMessage());
          }
          java.nio.file.Files.writeString(new File(dir, "expected" + suffix + ".error").toPath(), reason + "\n");
          System.out.println(dir.getName() + suffix + ": JAVA ERROR " + reason);
        }
      }
    }
    if (failed > 0)
    {
      System.out.println(failed + " case(s) failed in Java");
    }
  }

  /**
   * The case itself (suffix "") and each of its "variants", a comma-separated
   * list of key=value overrides such as "order=FILE, rotary_diameter_mm=100";
   * a variant writes expected@key-value.ltt (see tests/java_parity.rs).
   */
  private static Map<String, Properties> variants(Properties c)
  {
    Map<String, Properties> all = new java.util.LinkedHashMap<>();
    all.put("", c);
    for (String v : c.getProperty("variants", "").split(","))
    {
      String[] kv = v.trim().split("=", 2);
      if (kv.length != 2)
      {
        continue;
      }
      Properties p = (Properties) c.clone();
      p.setProperty(kv[0].trim(), kv[1].trim());
      // File-name safe on every system (no ':' on Windows), as in java_parity.rs.
      all.put(("@" + kv[0].trim() + "-" + kv[1].trim()).replaceAll("[^A-Za-z0-9@._-]", "_"), p);
    }
    return all;
  }

  /** A profile for one operation, its first LaserProperty and the job name prefix. */
  private static final class Setup
  {
    LaserProfile profile;
    LaserProperty property;
    String prefix;
  }

  private static Setup setup(String operation, Properties c, LaserCutter cutter) throws Exception
  {
    Setup s = new Setup();
    switch (operation)
    {
      case "cut":
      case "mark":
      {
        VectorProfile vp = new VectorProfile();
        vp.setIsCut("cut".equals(operation));
        // FILE, NEAREST, INNER_FIRST (default) or SMALLEST_FIRST.
        vp.setOrderStrategy(de.thomas_oster.liblasercut.vectoroptimizers.VectorOptimizer.OrderStrategy.valueOf(c.getProperty("order", "INNER_FIRST")));
        // "Nur Umriss": only the outline of all objects together.
        vp.setUseOutline(Boolean.parseBoolean(c.getProperty("outline", "false")));
        s.profile = vp;
        s.property = cutter.getLaserPropertyForVectorPart();
        s.prefix = "cut".equals(operation) ? "Cut" : "Mark";
        break;
      }
      case "engrave":
      {
        RasterProfile rp = new RasterProfile();
        // Class name in de.thomas_oster.liblasercut.dithering, e.g. FloydSteinberg.
        rp.setDitherAlgorithm((DitheringAlgorithm) Class.forName("de.thomas_oster.liblasercut.dithering." + c.getProperty("dither", "FloydSteinberg")).getDeclaredConstructor().newInstance());
        rp.setColorShift(Integer.parseInt(c.getProperty("color_shift", "0")));
        rp.setInvertColors(Boolean.parseBoolean(c.getProperty("invert", "false")));
        s.profile = rp;
        s.property = rasterProperty(cutter, c);
        s.prefix = "Engrav";
        break;
      }
      case "engrave3d":
      {
        s.profile = new Raster3dProfile();
        s.property = cutter.getLaserPropertyForRaster3dPart();
        s.prefix = "Eng3D";
        break;
      }
      default:
        throw new IllegalArgumentException("unknown operation " + operation);
    }
    s.profile.setDPI(500);
    return s;
  }

  private static LaserProperty rasterProperty(LaserCutter cutter, Properties c)
  {
    LaserProperty property = cutter.getLaserPropertyForRasterPart();
    property.setProperty("engrave unidirectional", Boolean.parseBoolean(c.getProperty("unidirectional", "false")));
    property.setProperty("engrave bottom up", Boolean.parseBoolean(c.getProperty("bottom_up", "false")));
    return property;
  }

  /** "power:speed[:passes]": one LaserProperty per pass, like VisiCut's GUI. */
  private static List<LaserProperty> passes(LaserProperty base, String values)
  {
    String[] v = values.trim().split(":");
    LaserProperty p = base.clone();
    p.setProperty("power", Float.parseFloat(v[0]));
    p.setProperty("speed", Float.parseFloat(v[1]));
    List<LaserProperty> result = new LinkedList<>();
    for (int i = 0; i < (v.length > 2 ? Integer.parseInt(v[2]) : 1); i++)
    {
      result.add(p.clone());
    }
    return result;
  }

  private static void generate(LaserDevice device, File dir, Properties c, String suffix) throws Exception
  {
    LaserCutter cutter = device.getLaserCutter();
    Setup base = setup(c.getProperty("operation"), c, cutter);
    LaserProfile profile = base.profile;
    LaserProperty property = base.property;
    property.setProperty("power", Float.parseFloat(c.getProperty("power", "20")));
    property.setProperty("speed", Float.parseFloat(c.getProperty("speed", "100")));
    // VisiCut has no pass counter: every further LaserProperty of a profile
    // processes the same objects once more.
    List<LaserProperty> props = new LinkedList<>();
    int passes = Integer.parseInt(c.getProperty("passes", "1"));
    for (int i = 0; i < passes; i++)
    {
      props.add(property.clone());
    }
    // Further parameter sets: "power:speed:passes;power:speed:passes".
    for (String set : c.getProperty("additional", "").split(";"))
    {
      if (!set.isBlank())
      {
        props.addAll(passes(property, set));
      }
    }

    VisicutModel model = new VisicutModel();
    model.setSelectedLaserDevice(device);
    // Added to the focus of every LaserProperty (useThicknessAsFocusOffset).
    model.setMaterialThickness(Float.parseFloat(c.getProperty("thickness_mm", "0")));
    // Rotary engraving: Y becomes the rotation of a cylinder of this diameter.
    if (c.getProperty("rotary_diameter_mm") != null)
    {
      model.setRotaryAxisEnabled(true);
      model.setRotaryAxisDiameterMm(Double.parseDouble(c.getProperty("rotary_diameter_mm")));
    }
    List<String> warnings = new ArrayList<>();
    // Profiles compare equal by their settings; each mapping needs its own.
    Map<LaserProfile, List<LaserProperty>> propmap = new java.util.IdentityHashMap<>();
    if (c.getProperty("svg").toLowerCase().endsWith(".plf"))
    {
      // A VisiCut project: positions and mappings come from the file, the
      // laser values ("settings", like "mappings") of every mapping with a
      // profile in part and mapping order, as VisiCut builds the job.
      de.thomas_oster.visicut.model.PlfFile plf = model.loadPlfFile(de.thomas_oster.visicut.managers.MappingManager.getInstance(), new File(dir, c.getProperty("svg")), warnings);
      String[] settings = c.getProperty("settings", "").split(";");
      int k = 0;
      for (PlfPart p : plf)
      {
        model.getPlfFile().add(p);
        if (p.getMapping() == null)
        {
          continue;
        }
        for (Mapping m : p.getMapping())
        {
          LaserProfile pr = m.getProfile();
          if (pr == null)
          {
            continue;
          }
          LaserProperty first = pr instanceof Raster3dProfile ? cutter.getLaserPropertyForRaster3dPart()
            : pr instanceof RasterProfile ? rasterProperty(cutter, c) : cutter.getLaserPropertyForVectorPart();
          propmap.put(pr, passes(first, settings[k++]));
        }
      }
    }
    else
    {
      PlfPart part = model.loadGraphicFile(new File(dir, c.getProperty("svg")), warnings);
      double x = Double.parseDouble(c.getProperty("x_mm"));
      double y = Double.parseDouble(c.getProperty("y_mm"));
      // "place=bbox": the top left corner of the geometry (without strokes)
      // goes to x/y, like placing a part in the GUI, instead of the document origin.
      if ("bbox".equals(c.getProperty("place")))
      {
        java.awt.geom.Rectangle2D bounds = geometryBounds(part.getGraphicObjects());
        x -= bounds.getX();
        y -= bounds.getY();
      }
      AffineTransform t = AffineTransform.getTranslateInstance(x, y);
      // Optional "scale": the part is resized like in VisiCut's GUI (uniform).
      double scale = Double.parseDouble(c.getProperty("scale", "1"));
      if (scale != 1)
      {
        t.scale(scale, scale);
      }
      t.concatenate(part.getGraphicObjects().getBasicTransform());
      // Natural size of the import in mm, for cases that let both sides derive it.
      java.awt.geom.Rectangle2D natural = part.getGraphicObjects().getBoundingBox();
      System.out.println(dir.getName() + ": java import bounds " + natural);
      part.getGraphicObjects().setTransform(t);
      MappingSet mapping = new MappingSet();
      String mappings = c.getProperty("mappings");
      if (mappings == null)
      {
        mapping.add(new Mapping(null, profile));
        propmap.put(profile, props);
      }
      else
      {
        // "[operation:] filters -> power:speed[:passes]; …" in mapping order, see README.
        for (String entry : mappings.split(";"))
        {
          String[] parts = entry.split("->");
          String filters = parts[0].trim();
          Setup s;
          java.util.regex.Matcher op = java.util.regex.Pattern.compile("^(cut|mark|engrave|engrave3d):\\s*(.*)$").matcher(filters);
          if (op.matches())
          {
            filters = op.group(2);
            s = setup(op.group(1), c, cutter);
          }
          else
          {
            s = setup(c.getProperty("operation"), c, cutter);
          }
          mapping.add(new Mapping(filterSet(filters), s.profile));
          propmap.put(s.profile, passes(s.property, parts[1]));
        }
      }
      part.setMapping(mapping);
      model.getPlfFile().add(part);
    }

    // "single_job=true": all operations in one job, named like VisiCutR's.
    String prefix = Boolean.parseBoolean(c.getProperty("single_job", "false")) ? "Job" : base.prefix;
    String name = jobName(prefix, c.getProperty("name"));
    File out = new File(dir, "expected" + suffix + ".ltt");
    new File(dir, "expected" + suffix + ".error").delete();
    model.saveJob(name, out, null, propmap);
    for (String w : warnings)
    {
      System.err.println(dir.getName() + ": " + w);
    }
    System.out.println(dir.getName() + suffix + ": " + out.length() + " bytes as " + name);
  }

  /**
   * "*": everything else (no FilterSet); "all": everything (empty FilterSet);
   * otherwise filters joined by "&", each "[!]Attribute=value" or
   * "[!]Attribute<=value" with VisiCut's attribute names. Colours become
   * java.awt.Color, stroke widths Double, like SVGShape's attribute values.
   */
  static de.thomas_oster.visicut.model.mapping.FilterSet filterSet(String spec)
  {
    if ("*".equals(spec))
    {
      return null;
    }
    de.thomas_oster.visicut.model.mapping.FilterSet set = new de.thomas_oster.visicut.model.mapping.FilterSet();
    if ("all".equals(spec))
    {
      return set;
    }
    for (String f : spec.split("&"))
    {
      f = f.trim();
      boolean inverted = f.startsWith("!");
      if (inverted)
      {
        f = f.substring(1);
      }
      boolean compare = f.contains("<=");
      String[] kv = f.split(compare ? "<=" : "=", 2);
      String attribute = kv[0].trim();
      String text = kv[1].trim();
      Object value;
      if (attribute.equals("Stroke Width"))
      {
        value = Double.parseDouble(text);
      }
      else if (attribute.endsWith("Color") && text.startsWith("#"))
      {
        value = java.awt.Color.decode(text);
      }
      else
      {
        value = text;
      }
      de.thomas_oster.visicut.model.mapping.MappingFilter filter = new de.thomas_oster.visicut.model.mapping.MappingFilter(attribute, value);
      filter.setInverted(inverted);
      filter.setCompare(compare);
      set.add(filter);
    }
    return set;
  }

  /** Union of the objects' geometry bounds in mm (basic transform, no strokes). */
  private static java.awt.geom.Rectangle2D geometryBounds(de.thomas_oster.visicut.model.graphicelements.GraphicSet objects)
  {
    java.awt.geom.Rectangle2D result = null;
    for (de.thomas_oster.visicut.model.graphicelements.GraphicObject e : objects)
    {
      java.awt.Shape shape = (e instanceof de.thomas_oster.visicut.model.graphicelements.ShapeObject)
        ? ((de.thomas_oster.visicut.model.graphicelements.ShapeObject) e).getShape() : e.getBoundingBox();
      java.awt.geom.Rectangle2D bb = objects.getBasicTransform().createTransformedShape(shape).getBounds2D();
      result = result == null ? bb : result.createUnion(bb);
    }
    return result == null ? new java.awt.geom.Rectangle2D.Double() : result;
  }

  /** Same device job name as VisiCutR (prefix, ASCII, at most 15 characters). */
  private static String jobName(String prefix, String name)
  {
    StringBuilder b = new StringBuilder();
    for (char ch : (prefix + "_" + name).toCharArray())
    {
      if (b.length() == 15)
      {
        break;
      }
      boolean ok = (ch < 128 && Character.isLetterOrDigit(ch)) || ch == '-' || ch == '_';
      b.append(ok ? ch : '_');
    }
    return b.toString();
  }
}
