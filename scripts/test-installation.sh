#!/bin/bash

# Test installation scripts without modifying the system
# This script validates that all paths are correctly resolved and files exist

set -e  # Exit on any error

TEST_PASSED=0
TEST_FAILED=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print test results
print_test() {
    local test_name="$1"
    local result="$2"
    local message="$3"

    if [ "$result" = "PASS" ]; then
        echo -e "${GREEN}✅ PASS${NC} $test_name"
        ((TEST_PASSED++))
    else
        echo -e "${RED}❌ FAIL${NC} $test_name: $message"
        ((TEST_FAILED++))
    fi
}

# Function to check if file exists
check_file() {
    local file="$1"
    local description="$2"

    if [ -f "$file" ]; then
        print_test "$description" "PASS" ""
        return 0
    else
        print_test "$description" "FAIL" "File not found: $file"
        return 1
    fi
}

echo "🧪 Testing FerrisPad Installation Scripts"
echo "========================================"
echo ""

# Test 1: Path Resolution
echo "📁 Testing Path Resolution..."
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd .. && pwd)"
EXPECTED_PROJECT="ferrispad"

if [[ "$PROJECT_ROOT" == *"$EXPECTED_PROJECT" ]]; then
    print_test "Project root resolution" "PASS" ""
else
    print_test "Project root resolution" "FAIL" "Expected path ending with '$EXPECTED_PROJECT', got: $PROJECT_ROOT"
fi

echo "Project root: $PROJECT_ROOT"
echo ""

# Test 2: Required Files Existence
echo "📄 Testing Required Files..."
check_file "$PROJECT_ROOT/scripts/generate-icons.sh" "Generate icons script exists"
check_file "$PROJECT_ROOT/scripts/install-desktop.sh" "Install script exists"
check_file "$PROJECT_ROOT/scripts/uninstall-desktop.sh" "Uninstall script exists"
check_file "$PROJECT_ROOT/data/img/ferrispad-logo.png" "Source logo exists"
check_file "$PROJECT_ROOT/ferrispad.desktop" "Desktop file exists"
check_file "$PROJECT_ROOT/target/release/FerrisPad" "Binary exists" || echo "  ⚠️  Note: Run 'cargo build --release' to create the binary"
echo ""

# Test 3: Script Executability
echo "🔧 Testing Script Permissions..."
if [ -x "$PROJECT_ROOT/scripts/generate-icons.sh" ]; then
    print_test "Generate icons script is executable" "PASS" ""
else
    print_test "Generate icons script is executable" "FAIL" "Script not executable"
fi

if [ -x "$PROJECT_ROOT/scripts/install-desktop.sh" ]; then
    print_test "Install script is executable" "PASS" ""
else
    print_test "Install script is executable" "FAIL" "Script not executable"
fi
echo ""

# Test 4: Dependencies Check
echo "🔍 Testing Dependencies..."
if command -v convert >/dev/null 2>&1; then
    print_test "ImageMagick (convert) available" "PASS" ""
else
    print_test "ImageMagick (convert) available" "FAIL" "Install with: sudo apt-get install imagemagick"
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
    print_test "desktop-file-validate available" "PASS" ""
else
    print_test "desktop-file-validate available" "FAIL" "Install with: sudo apt-get install desktop-file-utils"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    print_test "gtk-update-icon-cache available" "PASS" ""
else
    print_test "gtk-update-icon-cache available" "FAIL" "Install with: sudo apt-get install gtk-update-icon-cache"
fi
echo ""

# Test 5: Desktop File Validation
echo "✅ Testing Desktop File Validation..."
if command -v desktop-file-validate >/dev/null 2>&1; then
    if desktop-file-validate "$PROJECT_ROOT/ferrispad.desktop" 2>/dev/null; then
        print_test "Desktop file validation" "PASS" ""
    else
        # Check for warnings vs errors
        validation_output=$(desktop-file-validate "$PROJECT_ROOT/ferrispad.desktop" 2>&1)
        if echo "$validation_output" | grep -q "error:"; then
            print_test "Desktop file validation" "FAIL" "Validation errors found"
        else
            print_test "Desktop file validation (with warnings)" "PASS" "Has warnings but no errors"
        fi
    fi
else
    print_test "Desktop file validation" "SKIP" "desktop-file-validate not available"
fi
echo ""

# Test 6: Dry Run Icon Generation (if dependencies available)
echo "🎨 Testing Icon Generation (Dry Run)..."
if command -v convert >/dev/null 2>&1; then
    # Test if we can read the source image
    if convert "$PROJECT_ROOT/data/img/ferrispad-logo.png" -ping info: >/dev/null 2>&1; then
        print_test "Source image readable by ImageMagick" "PASS" ""

        # Test creating a small test icon
        TEST_DIR="/tmp/ferrispad_test_$$"
        mkdir -p "$TEST_DIR"

        if convert "$PROJECT_ROOT/data/img/ferrispad-logo.png" \
           -resize 32x32 \
           -gravity center \
           -extent 32x32 \
           "$TEST_DIR/test_icon.png" 2>/dev/null; then
            print_test "Icon generation test" "PASS" ""
            rm -rf "$TEST_DIR"
        else
            print_test "Icon generation test" "FAIL" "Cannot generate test icon"
        fi
    else
        print_test "Source image readable" "FAIL" "Cannot read source image"
    fi
else
    print_test "Icon generation test" "SKIP" "ImageMagick not available"
fi
echo ""

# Test 7: Path Consistency Check
echo "🔍 Testing Path Consistency..."
INSTALL_SCRIPT_PROJECT_ROOT=$(grep "PROJECT_ROOT=" "$PROJECT_ROOT/scripts/install-desktop.sh" | head -1)
GENERATE_SCRIPT_PROJECT_ROOT=$(grep "PROJECT_ROOT=" "$PROJECT_ROOT/scripts/generate-icons.sh" | head -1)

if [ "$INSTALL_SCRIPT_PROJECT_ROOT" = "$GENERATE_SCRIPT_PROJECT_ROOT" ]; then
    print_test "PROJECT_ROOT consistent between scripts" "PASS" ""
else
    print_test "PROJECT_ROOT consistent between scripts" "FAIL" "Inconsistent PROJECT_ROOT definitions"
fi

# Check if scripts reference the correct paths
if grep -q "PROJECT_ROOT.*icons" "$PROJECT_ROOT/scripts/install-desktop.sh"; then
    print_test "Install script uses PROJECT_ROOT for icons" "PASS" ""
else
    print_test "Install script uses PROJECT_ROOT for icons" "FAIL" "Install script not using PROJECT_ROOT correctly"
fi
echo ""

# Summary
echo "📊 Test Summary"
echo "==============="
echo -e "✅ Passed: ${GREEN}$TEST_PASSED${NC}"
echo -e "❌ Failed: ${RED}$TEST_FAILED${NC}"
echo -e "📊 Total:  $((TEST_PASSED + TEST_FAILED))"
echo ""

if [ $TEST_FAILED -eq 0 ]; then
    echo -e "${GREEN}🎉 All tests passed! The installation scripts should work correctly.${NC}"
    echo ""
    echo "Next steps:"
    echo "1. Run './scripts/generate-icons.sh' to create icons"
    echo "2. Run './install.sh' to install desktop integration"
    echo "3. Test with 'gtk-launch FerrisPad'"
    exit 0
else
    echo -e "${RED}⚠️  Some tests failed. Please fix the issues before using the installation scripts.${NC}"
    exit 1
fi